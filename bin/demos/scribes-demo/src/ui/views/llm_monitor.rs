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

use crate::ui::core::LLMInteraction;
use crate::ui::views::message_display::LLMInteractionRenderer;
use egui::Context;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub struct LLMMonitorWindow;

impl LLMMonitorWindow {
    pub fn new() -> Self {
        Self
    }

    pub fn show(&self, ctx: &Context, llm_interactions: &Arc<Mutex<VecDeque<LLMInteraction>>>) {
        egui::Window::new("LLM Monitor")
            .id(egui::Id::new("llm_monitor_window"))
            .default_size([800.0, 600.0])
            .show(ctx, |ui| {
                ui.label("Real-time LLM Interactions");
                ui.separator();

                if let Ok(interactions) = llm_interactions.try_lock() {
                    ui.horizontal(|ui| {
                        ui.label(format!("Total interactions: {}", interactions.len()));

                        let total_tokens: u32 =
                            interactions.iter().filter_map(|i| i.tokens_used).sum();
                        ui.label(format!("Total tokens: {total_tokens}"));

                        let total_cost: f64 = interactions.iter().filter_map(|i| i.cost).sum();
                        ui.label(format!("Total cost: ${total_cost:.4}"));
                    });
                    ui.separator();

                    LLMInteractionRenderer::show_interactions_list(
                        ui,
                        &interactions,
                        "llm_monitor_scroll",
                    );
                }
            });
    }
}
