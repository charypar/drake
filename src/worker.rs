use std::{fs, path::PathBuf};

use anyhow::{anyhow, bail};
use crossbeam::channel::{Receiver, Sender};
use tree_sitter::{Query, QueryCursor};

use crate::Package;

// A task for a parser worker to perform on a file with a path
pub enum Task {
    // Read Package.swift file and find package name
    PackageName(PathBuf, Sender<TaskResult>),
}

// A result from a parser worker
pub enum TaskResult {
    Package(Package),
}

pub struct Worker {
    task_rx: Receiver<Task>,
}

impl Worker {
    pub fn new(task_rx: Receiver<Task>) -> Self {
        Self { task_rx }
    }

    pub fn start(&self) -> anyhow::Result<()> {
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
