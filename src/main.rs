use std::{collections::HashMap, env, fs, path::PathBuf, thread};

use anyhow::{anyhow, bail};
use crossbeam::channel::{unbounded, Receiver, Sender};
use ignore::{types::TypesBuilder, WalkBuilder, WalkState};
use patricia_tree::GenericPatriciaMap;
use tree_sitter::{Node, Query, QueryCursor};

const NUM_WORKERS: usize = 4;

// Package definition
#[derive(Debug)]
struct Package {
    name: String,
    prefix: PathBuf,
}

// A task for a parser worker to perform on a file with a path
enum Task {
    // Read Package.swift file and find package name
    PackageName(PathBuf, Sender<TaskResult>),
}

// A result from a parser worker
enum TaskResult {
    Package(Package),
}

// PackageId is an offset into the list of known packages
type PackageId = usize;

// Search index
#[derive(Debug)]
struct Index {
    // Known packages. Offset is used as PackageId
    packages: Vec<Package>,
    // Find a package by name (e.g. for import)
    packages_by_name: HashMap<String, PackageId>,
    // Find a package by file path prefix
    packages_by_path: GenericPatriciaMap<String, PackageId>,
}

impl Index {
    fn new() -> Self {
        Self {
            packages: vec![],
            packages_by_name: HashMap::new(),
            packages_by_path: GenericPatriciaMap::new(),
        }
    }

    fn add_package(&mut self, package: Package) {
        let name = package.name.clone();
        let path = package
            .prefix
            .to_str()
            .expect("path should convert to string")
            .to_string();

        self.packages.push(package);
        let package_id = self.packages.len() - 1;

        self.packages_by_name.insert(name.clone(), package_id);
        self.packages_by_path.insert(path, package_id);

        println!("Indexed package {} under ID #{}", name, package_id);
    }
}

struct Drake {
    index: Index,
}

impl Drake {
    fn new() -> Self {
        Self {
            index: Index::new(),
        }
    }

    pub fn scan(&mut self, path: &str) -> anyhow::Result<()> {
        let mut builder = TypesBuilder::new();
        builder
            .add_defaults()
            .add("swiftpackage", "Package.swift")?;

        let matcher = builder.select("swiftpackage").build()?;
        let walk = WalkBuilder::new(path).types(matcher).build_parallel();

        let (task_tx, task_rx) = unbounded();
        let (result_tx, result_rx) = unbounded();

        walk.run(|| {
            let task_tx = task_tx.clone();
            let result_tx = result_tx.clone();

            Box::new(move |result| {
                if let Ok(dent) = result {
                    if let Some(ftype) = dent.file_type() {
                        if !ftype.is_dir() {
                            let task = Task::PackageName(dent.path().to_owned(), result_tx.clone());

                            task_tx.send(task).expect("couldn't send PackageName task");
                        }
                    }
                }

                WalkState::Continue
            })
        });

        drop(task_tx);
        drop(result_tx);

        for _ in 1..NUM_WORKERS {
            self.start_worker(task_rx.clone());
        }

        while let Ok(result) = result_rx.recv() {
            match result {
                TaskResult::Package(package) => self.index.add_package(package),
            }
        }

        println!("Done. Index: {:#?}", self.index);

        Ok(())
    }

    fn start_worker(&self, tasks: Receiver<Task>) {
        thread::spawn(|| {
            let worker = Worker::new(tasks);
            worker.start().expect("worker should run");
        });
    }
}

struct Worker {
    task_rx: Receiver<Task>,
}

impl Worker {
    fn new(task_rx: Receiver<Task>) -> Self {
        Self { task_rx }
    }

    fn start(&self) -> anyhow::Result<()> {
        while let Ok(task) = self.task_rx.recv() {
            match task {
                Task::PackageName(path, result_tx) => {
                    let source = fs::read_to_string(&path)?;
                    let name = get_package_name(&source)?;

                    let package = Package {
                        name,
                        prefix: path
                            .parent()
                            .ok_or_else(|| anyhow!("Package manifest has no parent directory??"))?
                            .to_owned(),
                    };

                    result_tx.send(TaskResult::Package(package))?;
                }
            }
        }

        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let path = &args[1];

    let mut drake = Drake::new();

    println!("Scanning path {}", path);
    drake.scan(path)
}

// Matches a package name in a Package.swift file
const PACKAGE_NAME_QUERY: &str = r#"
(call_expression
    (simple_identifier) @call_ident (#eq? @call_ident "Package")
    (call_suffix
        (value_arguments
            (value_argument
                (simple_identifier) @name_arg (#eq? @name_arg "name")
                (line_string_literal
                    (line_str_text) @package_name)))))
"#;

fn get_package_name(source: &str) -> anyhow::Result<String> {
    let mut parser = tree_sitter::Parser::new();
    let swift_language = tree_sitter_swift::language();
    parser
        .set_language(swift_language)
        .expect("failed to set swift language");

    let tree = parser.parse(source, None).expect("Couldn't parse the code");

    // FIXME: No need to do this every time
    let query = Query::new(swift_language, PACKAGE_NAME_QUERY).expect("failed parsing query");
    let mut query_cursor = QueryCursor::new();

    let first_match = query_cursor
        .matches(&query, tree.root_node(), source.as_bytes())
        .next()
        .ok_or_else(|| anyhow!("No matches for Package declaration"))?;

    for capture in first_match.captures {
        if capture.index == 2 {
            return Ok(source[capture.node.byte_range()].to_string());
        }
    }

    bail!("No matches for Package declaration")
}

fn print_node(node: Node, source: &str) {
    let mut depth = 0;
    let mut cursor = node.walk();

    loop {
        let node = cursor.node();

        print!("{}({}):", prefix(depth), node.kind());

        if node.child_count() < 1 {
            println!(
                " '{}' {} .. {}",
                &source[node.byte_range()],
                node.start_position(),
                node.end_position(),
            );
        } else {
            println!()
        }

        if cursor.goto_first_child() {
            depth += 1;
            continue;
        }

        if cursor.goto_next_sibling() {
            continue;
        }

        // can't go any deeper or further, go up

        loop {
            if !cursor.goto_parent() {
                // back at root
                return;
            }
            depth -= 1;

            if cursor.goto_next_sibling() {
                // There's another sibling to visit
                break;
            }
        }
    }
}

fn prefix(depth: usize) -> String {
    "  ".repeat(depth).to_string()
}
