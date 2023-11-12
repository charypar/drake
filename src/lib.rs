mod index;
mod worker;

use std::{fs, path::PathBuf, thread};

use crossbeam::channel::{unbounded, Receiver, Sender};
use ignore::{types::TypesBuilder, WalkBuilder, WalkParallel, WalkState};
use index::Index;
use worker::{to_sexp, Task, TaskResult, Worker};

use crate::worker::declarations;

// Package definition
#[derive(Debug)]
pub struct Package {
    name: String,
    prefix: PathBuf,
}

#[derive(Default)]
pub struct Drake {
    index: Index,
}

impl Drake {
    pub fn new() -> Self {
        Self {
            index: Index::new(),
        }
    }

    pub fn print(&mut self, path: &str, decl: bool, _refs: bool, full: bool) -> anyhow::Result<()> {
        let mut builder = TypesBuilder::new();
        builder.add_defaults();

        let matcher = builder.select("swift").build()?;
        let mut walk = WalkBuilder::new(path).types(matcher).build();

        while let Some(Ok(dir_entry)) = walk.next() {
            match dir_entry.file_type() {
                Some(ft) => {
                    if ft.is_dir() {
                        continue;
                    }
                }
                None => continue,
            }

            let path = dir_entry.path().to_string_lossy();

            let code = fs::read_to_string(dir_entry.path())?;

            if decl {
                for declaration in declarations(&code)? {
                    let loc = declaration.location;

                    match declaration.definition {
                        worker::Definition::Class { kind, name } => {
                            println!("{} {} in {} {}:{}", kind, name, path, loc.row, loc.column)
                        }
                        worker::Definition::Protocol { name } => {
                            println!("protocol {} in {} {}:{}", name, path, loc.row, loc.column)
                        }
                        worker::Definition::Extension { name } => {
                            println!("extension {} in {} {}:{}", name, path, loc.row, loc.column)
                        }
                    }
                }
            }

            if full {
                println!("## Parse tree\n");

                let s_expr = to_sexp(&code)?;
                println!("{s_expr}\n")
            }
        }

        Ok(())
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

        self.start_walk(walk, task_tx, result_tx);

        let n = num_cpus::get();
        for _ in 0..n {
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

    fn start_walk(&self, walk: WalkParallel, task_tx: Sender<Task>, result_tx: Sender<TaskResult>) {
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
    }

    fn start_worker(&self, tasks: Receiver<Task>) {
        thread::spawn(|| {
            let worker = Worker::new(tasks);
            worker.start().expect("worker should run");
        });
    }
}
