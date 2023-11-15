use std::{fs, path::PathBuf};

use anyhow::anyhow;
use crossbeam::channel::{Receiver, Sender};

use crate::parser::{Definition, Parser, Tree};
use crate::Package;

// A task for a parser worker to perform on a file with a path
pub enum Task {
    // Read Package.swift file and find package name
    PackageName(PathBuf, Sender<TaskResult>),
    Print {
        path: PathBuf,
        result_tx: Sender<TaskResult>,
        declarations: bool,
        references: bool,
        full: bool,
    },
}

// A result from a parser worker
pub enum TaskResult {
    Package(Package),
    PrintOutput(String),
}

pub struct Worker {
    task_rx: Receiver<Task>,
    parser: Parser,
}

impl Worker {
    pub fn new(task_rx: Receiver<Task>) -> Self {
        Self {
            task_rx,
            parser: Parser::new(),
        }
    }

    pub fn start(&self) -> anyhow::Result<()> {
        while let Ok(task) = self.task_rx.recv() {
            match task {
                Task::PackageName(path, result_tx) => {
                    let source = fs::read_to_string(&path)?;
                    let tree = self.parser.parse(&source)?;
                    let name = tree.package_name()?;

                    let package = Package {
                        name: name.to_string(),
                        prefix: path
                            .parent()
                            .ok_or_else(|| anyhow!("Package manifest has no parent directory??"))?
                            .to_owned(),
                    };

                    result_tx.send(TaskResult::Package(package))?;
                }
                Task::Print {
                    path,
                    result_tx,
                    declarations,
                    references,
                    full,
                } => {
                    let source = fs::read_to_string(&path)?;
                    let tree = self.parser.parse(&source)?;

                    let out = self.print(
                        &path.to_string_lossy(),
                        tree,
                        &source,
                        declarations,
                        references,
                        full,
                    )?;

                    result_tx.send(TaskResult::PrintOutput(out))?;
                }
            }
        }

        Ok(())
    }

    // TODO not great this...
    fn print(
        &self,
        path: &str,
        tree: Tree,
        code: &str,
        decl: bool,
        refs: bool,
        full: bool,
    ) -> anyhow::Result<String> {
        let mut out = String::new();

        out.push_str(&format!("# File {}\n", path));

        if decl {
            out.push_str("\n## Declarations\n\n");

            for declaration in tree.declarations(code)? {
                let loc = declaration.location;

                match declaration.definition {
                    Definition::Class { kind, name } => {
                        out.push_str(&format!(
                            "{} {} at {}:{}\n",
                            kind, name, loc.row, loc.column
                        ));
                    }
                    Definition::Protocol { name } => {
                        out.push_str(&format!(
                            "protocol {} at {}:{}\n",
                            name, loc.row, loc.column
                        ));
                    }
                    Definition::Extension { name } => {
                        out.push_str(&format!(
                            "extension {} at {}:{}\n",
                            name, loc.row, loc.column
                        ));
                    }
                }
            }
        }

        if refs {
            out.push_str("\n## References\n\n");

            for reference in tree.references(code)? {
                let loc = reference.location;
                let name = reference.name;

                out.push_str(&format!("{} at {}:{}\n", name, loc.row, loc.column));
            }
        }

        if full {
            out.push_str("\n## Parse tree\n\n");
            out.push_str(&format!("{}\n", tree));
        }

        Ok(out)
    }
}
