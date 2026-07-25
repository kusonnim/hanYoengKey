use std::{
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
};

use crate::hook::{complete_event, KeyboardEvent};

use super::coordinator::ApplicationConversionCoordinator;

pub(super) struct EventDispatcher {
    worker: Option<JoinHandle<()>>,
}

impl EventDispatcher {
    pub(super) fn start() -> std::io::Result<(Self, Sender<KeyboardEvent>)> {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("event-dispatcher".into())
            .spawn(move || {
                let coordinator = ApplicationConversionCoordinator::application();

                while let Ok(event) = receiver.recv() {
                    match event {
                        KeyboardEvent::HangulKeyPressed => {
                            let outcome = coordinator.process();
                            complete_event(outcome.handled());
                        }
                    }
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
