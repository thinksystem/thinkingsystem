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
use egui::Context;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub struct LearningSystemWindow;

impl LearningSystemWindow {
    pub fn new() -> Self {
        Self
    }

    pub fn show(&self, ctx: &Context, messages: &Arc<Mutex<VecDeque<ScribeMessage>>>) {
        egui::Window::new("Learning System")
            .id(egui::Id::new("learning_system_window"))
            .default_size([500.0, 300.0])
            .show(ctx, |ui| {
                ui.label("Q-Learning and Learning System Operations");
                ui.separator();

                if let Ok(messages) = messages.try_lock() {

                    let learning_messages: Vec<&ScribeMessage> = messages
                        .iter()
                        .filter(|msg| {
                            msg.scribe_name.contains("Learning") ||
                            msg.scribe_name.contains("Q-Learning") ||
                            msg.operation.contains("learning") ||
                            msg.operation.contains("choose_action") ||
                            msg.operation.contains("add_experience") ||
                            msg.operation.contains("update_q_values")
                        })
                        .collect();

                    if learning_messages.is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(150, 150, 150),
                            "No learning system operations yet. Learning systems activate when processing scenarios."
                        );
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("learning_system_scroll")
                            .show(ui, |ui| {
                                for (idx, message) in learning_messages.iter().rev().enumerate() {
                                    MessageRenderer::show_scribe_message(ui, message, idx, "");
                                }
                            });
                    }
                }
            });
    }
}
