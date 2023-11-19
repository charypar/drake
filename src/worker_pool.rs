use std::{path::Path, sync::Arc, thread};

use anyhow::Result;
use crossbeam::channel::{unbounded, Receiver};
use ignore::{WalkParallel, WalkState};

use crate::parser::Parser;

pub struct Results<T> {
    result_rx: Receiver<T>,
}

impl<T> Iterator for Results<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.result_rx.recv().ok()
    }
}

pub fn process_files<F, Output>(walk: WalkParallel, process_file: F) -> Results<Result<Output>>
where
    F: Fn(&Path, &Parser) -> Result<Output> + Send + Sync + 'static,
    Output: Send + 'static,
{
    let (task_tx, task_rx) = unbounded();

    walk.run(|| {
        let task_tx = task_tx.clone();

        Box::new(move |result| {
            if let Ok(dent) = result {
                if let Some(ftype) = dent.file_type() {
                    if !ftype.is_dir() {
                        task_tx
                            .send(dent.path().to_owned())
                            .expect("Cant't send task");
                    }
                }
            }

            WalkState::Continue
        })
    });

    drop(task_tx); // task_tx clones live in the walk threads

    let (result_tx, result_rx) = unbounded();

    let n = num_cpus::get();
    let work = Arc::new(process_file); // maybe there's a better way?

    for _ in 0..n {
        thread::spawn({
            let result_tx = result_tx.clone();
            let task_rx = task_rx.clone();
            let work = work.clone();

            move || {
                let parser = Parser::new();

                while let Ok(path) = task_rx.recv() {
                    let result = work(&path, &parser);

                    result_tx.send(result).expect("Can't send result");
                }
            }
        });
    }

    Results { result_rx }
}
