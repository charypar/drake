use std::{fs, path::PathBuf};

use anyhow::anyhow;
use crossbeam::channel::{Receiver, Sender};

use crate::{parser::Parser, Package};

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
            }
        }

        Ok(())
    }
}
