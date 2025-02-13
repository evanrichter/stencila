//! Utilities for displaying progress log entries on the command line

use std::{collections::HashMap, sync::Mutex};

use ansi_term::Color::Purple;
use events::ProgressEvent;
use linya::{Bar, Progress};

use common::{once_cell::sync::Lazy, serde_json};

pub static PROGRESS: Lazy<Mutex<Progress>> = Lazy::new(|| Mutex::new(Progress::new()));

pub static PROGRESS_BARS: Lazy<Mutex<HashMap<String, Bar>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn subscriber(_topic: String, event: serde_json::Value) {
    let mut progress = PROGRESS.lock().expect("Unable to lock progress");

    let ProgressEvent {
        parent,
        id,
        message,
        current,
        expected,
        ..
    } = serde_json::from_value(event).expect("Unable to deserialize event");

    // If the event is for a tasks with no parent then prefix line with PROG,
    // otherwise indent it, so it appears below parent
    let prefix = Purple
        .bold()
        .paint(if parent.is_none() { "PROG  " } else { "      " });

    // Should we draw / update a progress bar, or just print a message
    if let (Some(current), Some(expected)) = (current, expected) {
        if let Some(id) = id {
            let mut bars = PROGRESS_BARS.lock().expect("Unable to lock progress bars");

            // Get the current bar for this id, or create a new one
            let bar = match bars.get(&id) {
                Some(bar) => bar,
                None => {
                    let msg = format!("{}{}", prefix, message.unwrap_or_default());

                    let bar = progress.bar(expected as usize, msg);
                    bars.insert(id.clone(), bar);
                    &bars[&id]
                }
            };

            // Set the bar's current value
            progress.set_and_draw(bar, current as usize)
        }
    } else if let Some(message) = message {
        // Just print the message
        eprintln!("{}{}", prefix, message);
    }
}
