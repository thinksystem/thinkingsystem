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

use crate::ui::core::ScribeMessage;
use crate::ui::views::message_display::MessageRenderer;
use egui::{Context, TextEdit};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub struct ScribeWindow {
    title: String,
    scribe_filter: String,
    filter_input: String,
    auto_scroll: bool,
}

impl ScribeWindow {
    pub fn new(title: String, scribe_filter: String) -> Self {
        Self {
            title,
            scribe_filter,
            filter_input: String::new(),
            auto_scroll: true,
        }
    }

    pub fn show(
        &mut self,
        ctx: &Context,
        messages: &Arc<Mutex<VecDeque<ScribeMessage>>>,
        window_id: &str,
    ) {
        egui::Window::new(&self.title)
            .id(egui::Id::new(window_id))
            .default_size([600.0, 400.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    ui.add(
                        TextEdit::singleline(&mut self.filter_input)
                            .id(egui::Id::new(format!("{window_id}_filter")))
                            .hint_text("Filter messages..."),
                    );
                    ui.checkbox(&mut self.auto_scroll, "Auto-scroll");
                });
                ui.separator();

                if let Ok(messages) = messages.try_lock() {
                    let effective_filter = if self.filter_input.is_empty() {
                        &self.scribe_filter
                    } else {
                        &self.filter_input
                    };

                    MessageRenderer::show_messages_list(
                        ui,
                        &messages,
                        effective_filter,
                        &format!("{window_id}_scroll"),
                    );
                }
            });
    }
}
