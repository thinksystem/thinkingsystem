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

use crate::ui::core::{LLMInteraction, ScribeMessage};
use egui::{Color32, ScrollArea, Ui};
use serde_json::Value;
use std::collections::VecDeque;

pub struct MessageRenderer;

impl MessageRenderer {
    pub fn show_scribe_message(ui: &mut Ui, message: &ScribeMessage, idx: usize, filter: &str) {
        if !filter.is_empty()
            && !message
                .scribe_name
                .to_lowercase()
                .contains(&filter.to_lowercase())
        {
            return;
        }

        let time_str = message.timestamp.format("%H:%M:%S%.3f").to_string();
        let processing_time_text = message
            .processing_time_ms
            .map(|ms| format!(" ({ms}ms)"))
            .unwrap_or_default();

        let (bg_color, text_color) = match message.scribe_name.as_str() {
            name if name.contains("Knowledge") => (Color32::from_rgb(40, 60, 80), Color32::WHITE),
            name if name.contains("Data") => (Color32::from_rgb(60, 80, 40), Color32::WHITE),
            name if name.contains("Identity") => (Color32::from_rgb(80, 40, 60), Color32::WHITE),
            name if name.contains("Q-Learning") => (Color32::from_rgb(80, 60, 40), Color32::WHITE),
            _ => (Color32::from_rgb(50, 50, 50), Color32::WHITE),
        };

        egui::Frame::NONE
            .fill(bg_color)
            .inner_margin(8.0)
            .corner_radius(4.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        text_color,
                        format!("#{} [{}] {}", idx, time_str, message.scribe_name),
                    );
                    if message.is_llm_call {
                        ui.colored_label(Color32::YELLOW, "🤖 LLM");
                    }
                    ui.colored_label(Color32::LIGHT_GRAY, &processing_time_text);
                });

                ui.colored_label(text_color, format!("Operation: {}", message.operation));

                if !message.input.is_null() && message.input != Value::Null {
                    egui::CollapsingHeader::new("Input")
                        .id_salt(format!("input_collapse_{idx}"))
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut format!("{:#}", message.input))
                                    .id(egui::Id::new(format!("input_text_{idx}")))
                                    .desired_rows(3)
                                    .interactive(false),
                            );
                        });
                }

                if let Some(output) = &message.output {
                    if !output.is_null() {
                        egui::CollapsingHeader::new("Output")
                            .id_salt(format!("output_collapse_{idx}"))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut format!("{output:#}"))
                                        .id(egui::Id::new(format!("output_text_{idx}")))
                                        .desired_rows(3)
                                        .interactive(false),
                                );
                            });
                    }
                }
            });

        ui.separator();
    }

    pub fn show_messages_list(
        ui: &mut Ui,
        messages: &VecDeque<ScribeMessage>,
        filter: &str,
        scroll_source: &str,
    ) {
        ScrollArea::vertical()
            .id_salt(format!("messages_scroll_{scroll_source}"))
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for (idx, message) in messages.iter().rev().enumerate() {
                    Self::show_scribe_message(ui, message, idx, filter);
                }
            });
    }
}

pub struct LLMInteractionRenderer;

impl LLMInteractionRenderer {
    pub fn show_llm_interaction(ui: &mut Ui, interaction: &LLMInteraction, idx: usize) {
        let time_str = interaction.timestamp.format("%H:%M:%S%.3f").to_string();

        egui::Frame::NONE
            .fill(Color32::from_rgb(30, 40, 50))
            .inner_margin(8.0)
            .corner_radius(4.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        Color32::LIGHT_BLUE,
                        format!(
                            "#{} [{}] {} → {}",
                            idx, time_str, interaction.scribe_name, interaction.provider
                        ),
                    );

                    if let Some(tokens) = interaction.tokens_used {
                        ui.colored_label(Color32::YELLOW, format!("📊 {tokens} tokens"));
                    }

                    if let Some(cost) = interaction.cost {
                        ui.colored_label(Color32::GREEN, format!("💰 ${cost:.4}"));
                    }
                });

                egui::CollapsingHeader::new("Prompt")
                    .id_salt(format!("prompt_collapse_{idx}"))
                    .show(ui, |ui| {
                        ScrollArea::vertical()
                            .id_salt(format!("prompt_scroll_{idx}"))
                            .max_height(200.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut interaction.prompt.clone())
                                        .id(egui::Id::new(format!("prompt_text_{idx}")))
                                        .interactive(false)
                                        .desired_width(f32::INFINITY),
                                );
                            });
                    });

                egui::CollapsingHeader::new("Response")
                    .id_salt(format!("response_collapse_{idx}"))
                    .show(ui, |ui| {
                        ScrollArea::vertical()
                            .id_salt(format!("response_scroll_{idx}"))
                            .max_height(300.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut interaction.response.clone())
                                        .id(egui::Id::new(format!("response_text_{idx}")))
                                        .interactive(false)
                                        .desired_width(f32::INFINITY),
                                );
                            });
                    });
            });

        ui.separator();
    }

    pub fn show_interactions_list(
        ui: &mut Ui,
        interactions: &VecDeque<LLMInteraction>,
        scroll_source: &str,
    ) {
        ScrollArea::vertical()
            .id_salt(format!("interactions_scroll_{scroll_source}"))
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for (idx, interaction) in interactions.iter().rev().enumerate() {
                    Self::show_llm_interaction(ui, interaction, idx);
                }
            });
    }
}
