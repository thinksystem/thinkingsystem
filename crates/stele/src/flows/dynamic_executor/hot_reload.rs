// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2024 Jonathan Lee
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License version 3
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see https://www.gnu.org/licenses/.

use crate::blocks::rules::BlockError;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::{mpsc, Mutex};
pub struct HotReloadManager {
    watcher: RecommendedWatcher,
    event_receiver: Mutex<std::sync::mpsc::Receiver<Event>>,
}
impl HotReloadManager {
    pub fn new() -> Result<Self, BlockError> {
        let (tx, rx) = mpsc::channel();
        let watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        })
        .map_err(|e| BlockError::ProcessingError(format!("Failed to create watcher: {e}")))?;
        Ok(Self {
            watcher,
            event_receiver: Mutex::new(rx),
        })
    }
    pub fn watch_file(&mut self, path: &str) -> Result<(), BlockError> {
        self.watcher
            .watch(Path::new(path), RecursiveMode::NonRecursive)
            .map_err(|e| BlockError::ProcessingError(e.to_string()))
    }
    pub fn unwatch_file(&mut self, path: &str) -> Result<(), BlockError> {
        self.watcher
            .unwatch(Path::new(path))
            .map_err(|e| BlockError::ProcessingError(e.to_string()))
    }
    pub fn get_pending_events(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        if let Ok(receiver) = self.event_receiver.lock() {
            while let Ok(event) = receiver.try_recv() {
                events.push(event);
            }
        }
        events
    }
    pub fn extract_function_name_from_path(path: &std::path::Path) -> Option<String> {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    }
    pub fn should_reload_file(event: &Event) -> bool {
        matches!(event.kind, EventKind::Modify(_))
    }
    pub fn is_new_file(event: &Event) -> bool {
        matches!(event.kind, EventKind::Create(_))
    }
}
