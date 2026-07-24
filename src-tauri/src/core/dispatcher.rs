use std::{
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
};

use crate::{
    hook::KeyboardEvent,
    selection::{SelectionResult, SelectionService},
};

pub(super) struct EventDispatcher {
    worker: Option<JoinHandle<()>>,
}

impl EventDispatcher {
    pub(super) fn start() -> std::io::Result<(Self, Sender<KeyboardEvent>)> {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("event-dispatcher".into())
            .spawn(move || {
                let selection_service = SelectionService::new();

                while let Ok(event) = receiver.recv() {
                    match event {
                        KeyboardEvent::HangulKeyPressed => {
                            print_selection(selection_service.get_selected_text());
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

fn print_selection(result: SelectionResult) {
    match result {
        SelectionResult::Success(text) => println!("Selected text: {text}"),
        SelectionResult::NoSelection => println!("Selected text: <none>"),
        SelectionResult::Unsupported => println!("Selected text: <unsupported>"),
        SelectionResult::Failure(error) => eprintln!("Selection failed: {error}"),
    }
}

impl Drop for EventDispatcher {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
