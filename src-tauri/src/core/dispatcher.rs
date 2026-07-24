use std::{
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
};

use crate::hook::KeyboardEvent;

pub(super) struct EventDispatcher {
    worker: Option<JoinHandle<()>>,
}

impl EventDispatcher {
    pub(super) fn start() -> std::io::Result<(Self, Sender<KeyboardEvent>)> {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("event-dispatcher".into())
            .spawn(move || {
                while let Ok(event) = receiver.recv() {
                    // Phase 2 has no subscribers. Console output provides a
                    // temporary observable endpoint for manual verification.
                    println!("Keyboard event: {event:?}");
                }
            })?;

        Ok((
            Self {
                worker: Some(worker),
            },
            sender,
        ))
    }
}

impl Drop for EventDispatcher {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
