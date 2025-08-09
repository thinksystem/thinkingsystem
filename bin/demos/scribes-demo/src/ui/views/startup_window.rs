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

use crate::scenario_generator::{GeneratedScenario, InteractiveScenarioGenerator};
use egui::{Context, TextEdit, Ui, Window};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;

pub struct StartupWindow {
    topic: String,
    generated_scenarios: Arc<Mutex<Vec<GeneratedScenario>>>,
    is_generating: Arc<Mutex<bool>>,
    rt: Handle,
    llm_status_checked: bool,
    is_local_llm_available: bool,
    last_status_check: Option<Instant>,
}

impl StartupWindow {
    pub fn new(rt: Handle) -> Self {
        Self {
            topic: "AI systems collaborating in crisis management".to_string(),
            generated_scenarios: Arc::new(Mutex::new(Vec::new())),
            is_generating: Arc::new(Mutex::new(false)),
            rt,
            llm_status_checked: false,
            is_local_llm_available: false,
            last_status_check: None,
        }
    }

    pub fn show(
        &mut self,
        ctx: &Context,
        is_open: &mut bool,
        generator: &Arc<InteractiveScenarioGenerator>,
    ) -> Option<String> {
        let mut all_scenarios: Option<String> = None;

        Window::new("Generate Scenarios")
            .open(is_open)
            .collapsible(false)
            .resizable(true)
            .default_width(650.0)
            .default_height(500.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                all_scenarios = self.ui(ui, generator);
            });

        if all_scenarios.is_some() {
            *is_open = false;
        }

        all_scenarios
    }

    fn ui(&mut self, ui: &mut Ui, generator: &Arc<InteractiveScenarioGenerator>) -> Option<String> {
        let mut choice: Option<String> = None;

        ui.add_space(10.0);
        ui.heading("STELE Scribes System Demo");
        ui.add_space(5.0);
        ui.label("Create scenarios based on your topic of interest, then proceed with all generated scenarios.");

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label("LLM Status:");

            let now = Instant::now();
            let should_recheck = match self.last_status_check {
                None => true,
                Some(last) => now.duration_since(last) > Duration::from_secs(5),
            };

            if should_recheck {
                self.llm_status_checked = false;
                self.last_status_check = Some(now);
            }

            let current_status = self.rt.block_on(generator.get_current_status());

            let (status_color, status_text) = match current_status {
                crate::scenario_generator::LLMStatus::Idle => {
                    if !self.llm_status_checked {
                        self.check_llm_status(generator);
                        (
                            egui::Color32::from_rgb(100, 100, 100),
                            "Checking connection...",
                        )
                    } else if self.is_local_llm_available {
                        (
                            egui::Color32::from_rgb(0, 150, 0),
                            "Local LLM (Ollama) Ready",
                        )
                    } else {
                        (
                            egui::Color32::from_rgb(255, 150, 0),
                            "External LLM (Anthropic/OpenAI)",
                        )
                    }
                }
                crate::scenario_generator::LLMStatus::LocalProcessing => (
                    egui::Color32::from_rgb(0, 120, 255),
                    "Using Local LLM (Ollama)",
                ),
                crate::scenario_generator::LLMStatus::FallbackProcessing => (
                    egui::Color32::from_rgb(255, 150, 0),
                    "Using Cloud LLM (Fallback)",
                ),
                crate::scenario_generator::LLMStatus::Failed => {
                    (egui::Color32::from_rgb(255, 0, 0), "LLM Connection Failed")
                }
            };

            ui.colored_label(status_color, status_text);
        });

        ui.add_space(15.0);

        ui.label("Topic:");
        ui.add_sized(
            [ui.available_width(), 25.0],
            TextEdit::singleline(&mut self.topic)
                .id_salt("topic_input")
                .hint_text("e.g., 'autonomous vehicles in smart cities'"),
        );

        ui.add_space(8.0);

        let is_generating = *self.is_generating.lock().unwrap();
        let has_generated_scenarios = !self.generated_scenarios.lock().unwrap().is_empty();

        let button_text = if is_generating {
            "Generating..."
        } else if has_generated_scenarios {
            "Regenerate Scenarios"
        } else {
            "Generate Scenarios"
        };

        let generate_button = egui::Button::new(button_text);
        if ui
            .add_sized([ui.available_width(), 35.0], generate_button)
            .clicked()
            && !is_generating
            && !self.topic.trim().is_empty()
        {
            self.generated_scenarios.lock().unwrap().clear();
            self.start_generation(generator);
        }

        if is_generating {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.spinner();

                let current_status = self.rt.block_on(generator.get_current_status());

                let (status_color, status_text) = match current_status {
                    crate::scenario_generator::LLMStatus::LocalProcessing => (
                        egui::Color32::from_rgb(0, 120, 255),
                        "Using Local LLM (Ollama)...",
                    ),
                    crate::scenario_generator::LLMStatus::FallbackProcessing => (
                        egui::Color32::from_rgb(255, 150, 0),
                        "Using Cloud LLM (Anthropic/OpenAI)...",
                    ),
                    crate::scenario_generator::LLMStatus::Failed => {
                        (egui::Color32::from_rgb(255, 0, 0), "Generation failed")
                    }
                    _ => (egui::Color32::from_rgb(100, 100, 100), "Generating..."),
                };

                ui.colored_label(status_color, status_text);
            });
        }

        let scenarios = self.generated_scenarios.lock().unwrap().clone();
        if !scenarios.is_empty() {
            ui.add_space(15.0);
            ui.separator();
            ui.strong(format!("Generated {} Scenarios:", scenarios.len()));
            ui.add_space(8.0);

            egui::ScrollArea::vertical()
                .id_salt("scenarios_scroll")
                .max_height(250.0)
                .show(ui, |ui| {
                    for (index, scenario) in scenarios.iter().enumerate() {
                        ui.push_id(format!("scenario_{index}"), |ui| {
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.set_min_width(ui.available_width() - 20.0);
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.strong(format!(
                                            "Scenario {}: {}",
                                            index + 1,
                                            scenario.name
                                        ));
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label(format!(
                                                    "{} | {}",
                                                    scenario.category, scenario.priority
                                                ));
                                            },
                                        );
                                    });

                                    ui.add_space(5.0);
                                    ui.label(&scenario.description);
                                });
                            });
                        });
                        ui.add_space(10.0);
                    }
                });

            ui.add_space(15.0);

            let proceed_button = egui::Button::new("Proceed with All Scenarios")
                .fill(egui::Color32::from_rgb(0, 120, 0));

            if ui
                .add_sized([ui.available_width(), 40.0], proceed_button)
                .clicked()
            {
                let all_scenarios_text = scenarios.iter().enumerate().map(|(index, scenario)| {
                    format!(
                        "=== SCENARIO {}\nName: {}\nDescription: {}\nCategory: {}\nPriority: {}\n\nData:\n{}\n\nOutcome:\n{}",
                        index + 1,
                        scenario.name,
                        scenario.description,
                        scenario.category,
                        scenario.priority,
                        serde_json::to_string_pretty(&scenario.data).unwrap_or_else(|_| "Invalid data".to_string()),
                        serde_json::to_string_pretty(&scenario.expected_outcome).unwrap_or_else(|_| "Invalid outcome".to_string())
                    )
                }).collect::<Vec<String>>().join("\n\n");

                choice = Some(all_scenarios_text);
            }
        }
        choice
    }

    fn check_llm_status(&mut self, generator: &Arc<InteractiveScenarioGenerator>) {
        if self.llm_status_checked {
            return;
        }

        self.llm_status_checked = true;

        let test_result = self.rt.block_on(async {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(2))
                .build()
                .unwrap();

            client.get("http://localhost:11434/api/tags").send().await
        });

        match test_result {
            Ok(response) if response.status().is_success() => {
                self.is_local_llm_available = true;
                tracing::info!("LLM status check - Local LLM (Ollama) is reachable");
            }
            _ => {
                self.is_local_llm_available = false;
                tracing::warn!(
                    "LLM status check - Local LLM (Ollama) not reachable, will use fallback"
                );
            }
        }
    }

    fn start_generation(&mut self, generator: &Arc<InteractiveScenarioGenerator>) {
        *self.is_generating.lock().unwrap() = true;
        let topic = self.topic.clone();
        let generator_clone = Arc::clone(generator);
        let generated_scenarios_clone = Arc::clone(&self.generated_scenarios);
        let is_generating_clone = Arc::clone(&self.is_generating);

        self.rt.spawn(async move {
            let result = generator_clone.generate_scenarios_for_topic(&topic).await;
            match result {
                Ok(scenarios) => {
                    *generated_scenarios_clone.lock().unwrap() = scenarios;
                }
                Err(e) => {
                    tracing::error!("Failed to generate scenarios: {e}");

                    let error_scenario = crate::scenario_generator::GeneratedScenario {
                        id: "error-001".to_string(),
                        name: "Generation Error".to_string(),
                        description: format!("Failed to generate scenarios: {e}"),
                        category: "error".to_string(),
                        priority: "high".to_string(),
                        data: serde_json::json!({
                            "error": true,
                            "message": "Please try a different topic or check your local LLM connection."
                        }),
                        expected_outcome: serde_json::json!({ "success": false, "error": true }),
                    };
                    *generated_scenarios_clone.lock().unwrap() = vec![error_scenario];
                }
            }
            *is_generating_clone.lock().unwrap() = false;
        });
    }
}
