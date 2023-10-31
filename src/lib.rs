mod index;
mod worker;

use std::{path::PathBuf, thread};

use crossbeam::channel::{unbounded, Receiver, Sender};
use ignore::{types::TypesBuilder, WalkBuilder, WalkParallel, WalkState};
use index::Index;
use worker::{Task, TaskResult, Worker};

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
