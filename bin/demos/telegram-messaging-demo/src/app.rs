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

use crate::cli::Args;
use crate::telegram_client::{TelegramClient, TelegramMessage};
use egui::{Color32, ComboBox, Context, RichText, ScrollArea, TextEdit, Ui};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use steel::messaging::insight::{
    ContentAnalysis, HybridConfig, HybridContentAnalyser, HybridContentAnalysis, NerAnalysisResult,
    NerConfig, ScoreDistribution, ScoringConfig,
};
use tokio::sync::mpsc;
use tracing::debug;

pub struct TelegramApp {
    token: String,
    chat_id: String,
    connected: bool,
    status_message: String,

    discovered_chats: Vec<(i64, String)>,
    selected_chat_id: Option<i64>,

    message_input: String,
    show_insights: bool,

    messages: Arc<Mutex<Vec<TelegramMessage>>>,
    insights: Arc<Mutex<Vec<(String, HybridContentAnalysis)>>>,

    hybrid_analyser: HybridContentAnalyser,
    score_distribution: ScoreDistribution,

    to_worker: Option<mpsc::UnboundedSender<WorkerMessage>>,
    from_worker: mpsc::UnboundedReceiver<AppMessage>,
    _from_worker_sender: mpsc::UnboundedSender<AppMessage>,

    runtime: tokio::runtime::Runtime,
}

enum WorkerMessage {
    SendMessage(i64, String),
    DiscoverChats,
}

enum AppMessage {
    Connected,
    ConnectionFailed(String),
    ChatsDiscovered(Vec<(i64, String)>),
    NewMessages(Vec<TelegramMessage>),
    MessageSent,
    Error(String),
}

impl TelegramApp {
    pub fn new(args: Args) -> Self {
        let (from_worker_sender, from_worker_receiver) = mpsc::unbounded_channel();

        let scoring_config = ScoringConfig::default();
        let ner_config = NerConfig::default();
        let hybrid_config = HybridConfig::default();
        let mut hybrid_analyser =
            HybridContentAnalyser::new(scoring_config, ner_config, hybrid_config);

        if let Err(e) = hybrid_analyser.initialise_ner() {
            tracing::warn!(
                "Failed to initialise NER model: {}. Entity extraction will be disabled.",
                e
            );
        }

        let score_distribution = ScoreDistribution::new(1000);

        Self {
            token: args.get_token().unwrap_or_default(),
            chat_id: args.get_chat_id().unwrap_or_default(),
            connected: false,
            status_message: "Enter bot token and connect to discover chats".to_string(),
            discovered_chats: Vec::new(),
            selected_chat_id: args.get_chat_id().and_then(|s| s.parse().ok()),
            message_input: String::new(),
            show_insights: true,
            messages: Arc::new(Mutex::new(Vec::new())),
            insights: Arc::new(Mutex::new(Vec::new())),
            hybrid_analyser,
            score_distribution,
            to_worker: None,
            from_worker: from_worker_receiver,
            _from_worker_sender: from_worker_sender,
            runtime: tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime"),
        }
    }

    fn connect(&mut self) {
        if self.token.trim().is_empty() {
            self.status_message = "Bot token cannot be empty".to_string();
            return;
        }

        self.status_message = "Connecting...".to_string();

        let token = self.token.clone();
        let from_worker = self._from_worker_sender.clone();
        let (to_worker_sender, mut to_worker_receiver) = mpsc::unbounded_channel();
        self.to_worker = Some(to_worker_sender);

        if let Ok(chat_id) = self.chat_id.parse::<i64>() {
            self.discovered_chats
                .push((chat_id, format!("Chat {chat_id} (from config)")));
            self.selected_chat_id = Some(chat_id);
        }

        self.runtime.spawn(async move {
            let mut client = TelegramClient::new(token);


            if let Err(e) = client.test_connection().await {
                let _ = from_worker.send(AppMessage::ConnectionFailed(format!("Connection failed: {e}")));
                return;
            }

            let _ = from_worker.send(AppMessage::Connected);

            let mut interval = tokio::time::interval(Duration::from_secs(3));
            loop {
                tokio::select! {
                    Some(msg) = to_worker_receiver.recv() => {
                        match msg {
                            WorkerMessage::SendMessage(chat_id, text) => {
                                match client.send_message(chat_id, &text).await {
                                    Ok(_) => {
                                        let _ = from_worker.send(AppMessage::MessageSent);
                                    }
                                    Err(e) => {
                                        let _ = from_worker.send(AppMessage::Error(format!("Failed to send: {e}")));
                                    }
                                }
                            }
                            WorkerMessage::DiscoverChats => {
                                let discovered = Self::discover_chats_from_updates(&mut client).await;
                                let _ = from_worker.send(AppMessage::ChatsDiscovered(discovered));
                            }
                        }
                    },
                    _ = interval.tick() => {
                        match client.get_updates().await {
                            Ok(messages) => {
                                if !messages.is_empty() {

                                    let mut discovered_chats = Vec::new();
                                    for msg in &messages {
                                        let display_name = if let Some(from_user) = &msg.from_user {
                                            format!("{from_user} (Private)")
                                        } else {
                                            format!("Chat {}", msg.chat_id)
                                        };
                                        discovered_chats.push((msg.chat_id, display_name));
                                    }


                                    if !discovered_chats.is_empty() {
                                        let _ = from_worker.send(AppMessage::ChatsDiscovered(discovered_chats));
                                    }


                                    let _ = from_worker.send(AppMessage::NewMessages(messages));
                                }
                            }
                            Err(e) => {
                                debug!("Failed to get updates: {e}");
                            }
                        }
                    }
                }
            }
        });
    }

    async fn discover_chats_from_updates(client: &mut TelegramClient) -> Vec<(i64, String)> {
        debug!("Discovering chats from recent updates...");

        match client.discover_chats().await {
            Ok(chats) => {
                debug!("Discovered {} chats", chats.len());
                chats
            }
            Err(e) => {
                debug!("Failed to discover chats: {e}");
                Vec::new()
            }
        }
    }

    fn send_message(&mut self) {
        if self.message_input.trim().is_empty() {
            return;
        }

        if let Some(chat_id) = self.selected_chat_id {
            if let Some(sender) = &self.to_worker {
                let text = self.message_input.clone();

                let outgoing_msg = TelegramMessage {
                    message_id: 0,
                    from_user: Some("You".to_string()),
                    chat_id,
                    text: text.clone(),
                    timestamp: chrono::Utc::now(),
                    is_outgoing: true,
                };

                if let Ok(mut messages) = self.messages.lock() {
                    messages.push(outgoing_msg);
                }

                let _ = sender.send(WorkerMessage::SendMessage(chat_id, text));
                self.message_input.clear();
            }
        } else {
            self.status_message = "Please select a chat first".to_string();
        }
    }

    fn handle_worker_messages(&mut self) {
        while let Ok(msg) = self.from_worker.try_recv() {
            match msg {
                AppMessage::Connected => {
                    self.connected = true;
                    self.status_message = "Connected! Discovering chats...".to_string();
                }
                AppMessage::ConnectionFailed(err) => {
                    self.connected = false;
                    self.status_message = err;
                }
                AppMessage::ChatsDiscovered(chats) => {
                    for (id, name) in chats {
                        if !self
                            .discovered_chats
                            .iter()
                            .any(|(existing_id, _)| *existing_id == id)
                        {
                            self.discovered_chats.push((id, name));
                        }
                    }

                    if self.selected_chat_id.is_none() && !self.discovered_chats.is_empty() {
                        self.selected_chat_id = Some(self.discovered_chats[0].0);
                    }

                    if self.discovered_chats.is_empty() {
                        self.status_message = "No chats found. Send a message to your bot first, then try reconnecting.".to_string();
                    } else {
                        self.status_message =
                            format!("Ready! Found {} chat(s)", self.discovered_chats.len());
                    }
                }
                AppMessage::NewMessages(new_msgs) => {
                    let messages_to_analyse: Vec<String> = new_msgs
                        .iter()
                        .filter(|msg| !msg.is_outgoing)
                        .map(|msg| msg.text.clone())
                        .collect();

                    if let Ok(mut messages) = self.messages.lock() {
                        messages.retain(|m| m.message_id != 0);

                        messages.extend(new_msgs);
                        messages.sort_by_key(|m| m.timestamp);
                        messages.dedup_by_key(|m| m.message_id);

                        if messages.len() > 100 {
                            let len = messages.len();
                            messages.drain(0..len - 100);
                        }
                    }

                    for text in messages_to_analyse {
                        self.analyse_message_insights(&text);
                    }
                }
                AppMessage::MessageSent => {}
                AppMessage::Error(err) => {
                    self.status_message = err;
                }
            }
        }
    }

    fn analyse_message_insights(&mut self, text: &str) {
        let analysis = match self
            .hybrid_analyser
            .analyse_hybrid(text, &mut self.score_distribution)
        {
            Ok(analysis) => analysis,
            Err(e) => {
                tracing::warn!(
                    "Hybrid analysis failed: {}. Falling back to basic analysis.",
                    e
                );

                HybridContentAnalysis {
                    syntactic_analysis: ContentAnalysis {
                        overall_risk_score: 0.0,
                        interesting_tokens: vec![],
                        requires_scribes_review: false,
                    },
                    ner_analysis: NerAnalysisResult {
                        entities: vec![],
                        overall_ner_score: 0.0,
                        processing_time_ms: 0.0,
                        text_truncated: false,
                    },
                    combined_risk_score: 0.0,
                    requires_scribes_review: false,
                    analysis_method: "Fallback".to_string(),
                }
            }
        };

        debug!(
            "Message analysis: combined_risk_score={}, entities={}, tokens={}, requires_review={}, method={}",
            analysis.combined_risk_score,
            analysis.ner_analysis.entities.len(),
            analysis.syntactic_analysis.interesting_tokens.len(),
            analysis.requires_scribes_review,
            analysis.analysis_method
        );

        if let Ok(mut insights) = self.insights.lock() {
            insights.push((text.to_string(), analysis));

            if insights.len() > 50 {
                insights.remove(0);
            }
        }
    }
}

impl eframe::App for TelegramApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.handle_worker_messages();
        ctx.request_repaint_after(Duration::from_millis(500));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Telegram Demo with Token Analysis");
            ui.separator();

            ui.group(|ui| {
                ui.heading("Connection");

                ui.horizontal(|ui| {
                    ui.label("Bot Token:");
                    ui.add(
                        TextEdit::singleline(&mut self.token)
                            .password(true)
                            .hint_text("Enter your bot token"),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("Chat ID (optional):");
                    ui.add(TextEdit::singleline(&mut self.chat_id).hint_text("Fallback chat ID"));
                });

                ui.horizontal(|ui| {
                    if ui.button("Connect").clicked() {
                        self.connect();
                    }

                    if self.connected && ui.button("Rediscover Chats").clicked() {
                        if let Some(sender) = &self.to_worker {
                            let _ = sender.send(WorkerMessage::DiscoverChats);
                        }
                    }

                    ui.label(&self.status_message);
                });

                if !self.discovered_chats.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label("Select Chat:");
                        let selected_text = self
                            .selected_chat_id
                            .and_then(|id| {
                                self.discovered_chats
                                    .iter()
                                    .find(|(chat_id, _)| *chat_id == id)
                            })
                            .map(|(_, name)| name.clone())
                            .unwrap_or_else(|| "Select a chat...".to_string());

                        ComboBox::from_id_salt("main_chat_selector")
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                for (chat_id, name) in &self.discovered_chats {
                                    ui.selectable_value(
                                        &mut self.selected_chat_id,
                                        Some(*chat_id),
                                        name,
                                    );
                                }
                            });

                        ui.separator();
                        ui.checkbox(&mut self.show_insights, "Show Token Analysis");
                    });
                }
            });

            ui.separator();

            if self.connected && self.selected_chat_id.is_some() {
                let available_height = ui.available_height() - 80.0;

                if self.show_insights {
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(ui.available_width() * 0.65, available_height),
                            egui::Layout::top_down(egui::Align::LEFT),
                            |ui| {
                                ui.group(|ui| {
                                    ui.heading("Messages");
                                    ScrollArea::vertical()
                                        .id_salt("messages_scroll_area")
                                        .stick_to_bottom(true)
                                        .max_height(available_height - 100.0)
                                        .show(ui, |ui| {
                                            self.render_messages_for_chat(ui);
                                        });
                                });
                            },
                        );

                        ui.separator();

                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(ui.available_width(), available_height),
                            egui::Layout::top_down(egui::Align::LEFT),
                            |ui| {
                                self.render_insights_panel(ui, available_height);
                            },
                        );
                    });
                } else {
                    ui.group(|ui| {
                        ScrollArea::vertical()
                            .id_salt("fullwidth_messages_scroll_area")
                            .stick_to_bottom(true)
                            .max_height(available_height)
                            .show(ui, |ui| {
                                self.render_messages_for_chat(ui);
                            });
                    });
                }

                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        let response = ui.add(
                            TextEdit::singleline(&mut self.message_input)
                                .hint_text("Type your message...")
                                .desired_width(ui.available_width() - 70.0),
                        );

                        if ui.button("Send").clicked()
                            || (response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        {
                            self.send_message();
                        }
                    });
                });
            } else {
                ui.centered_and_justified(|ui| {
                    if !self.connected {
                        ui.label("Please connect to start discovering chats");
                    } else {
                        ui.label("Select a chat to start messaging");
                    }
                });
            }
        });
    }
}

impl TelegramApp {
    fn render_messages_for_chat(&self, ui: &mut Ui) {
        if let Ok(messages) = self.messages.lock() {
            if messages.is_empty() {
                ui.colored_label(Color32::GRAY, "No messages yet. Send one!");
            } else {
                for msg in messages.iter() {
                    if msg.chat_id == self.selected_chat_id.unwrap() {
                        self.render_message(ui, msg);
                    }
                }
            }
        } else {
            ui.colored_label(Color32::RED, "Error loading messages");
        }
    }

    fn render_insights_panel(&self, ui: &mut Ui, available_height: f32) {
        ui.group(|ui| {
            ui.heading("Token Analysis");

            ScrollArea::vertical()
                .id_salt("insights_scroll_area")
                .max_height(available_height - 60.0)
                .show(ui, |ui| {
                    if let Ok(insights) = self.insights.lock() {
                        if insights.is_empty() {
                            ui.colored_label(Color32::GRAY, "No analysed messages yet.");
                            return;
                        }

                        for (idx, (text, analysis)) in insights.iter().rev().enumerate() {
                            let frame = egui::Frame::NONE
                                .fill(egui::Color32::from_rgb(248, 249, 250))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::LIGHT_GRAY))
                                .corner_radius(6.0)
                                .inner_margin(8.0);

                            ui.push_id(format!("insight_{idx}"), |ui| {
                                frame.show(ui, |ui| {
                                    let preview = if text.len() > 40 {
                                        format!("{}...", &text[..40])
                                    } else {
                                        text.clone()
                                    };

                                    ui.label(
                                        egui::RichText::new(preview)
                                            .small()
                                            .italics()
                                            .color(egui::Color32::DARK_GRAY),
                                    );

                                    ui.separator();

                                    let risk_colour = if analysis.combined_risk_score > 0.7 {
                                        egui::Color32::RED
                                    } else if analysis.combined_risk_score > 0.4 {
                                        egui::Color32::from_rgb(255, 140, 0)
                                    } else {
                                        egui::Color32::from_rgb(34, 139, 34)
                                    };

                                    ui.horizontal(|ui| {
                                        ui.label("Combined Risk:");
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{:.2}",
                                                analysis.combined_risk_score
                                            ))
                                            .color(risk_colour)
                                            .strong(),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "({})",
                                                analysis.analysis_method
                                            ))
                                            .small()
                                            .color(egui::Color32::GRAY),
                                        );
                                    });

                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("Syntactic:").small());
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{:.2}",
                                                analysis.syntactic_analysis.overall_risk_score
                                            ))
                                            .small()
                                            .color(egui::Color32::BLUE),
                                        );
                                        ui.separator();
                                        ui.label(egui::RichText::new("NER:").small());
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{:.2}",
                                                analysis.ner_analysis.overall_ner_score
                                            ))
                                            .small()
                                            .color(egui::Color32::GREEN),
                                        );
                                    });

                                    if !analysis.ner_analysis.entities.is_empty() {
                                        ui.add_space(4.0);
                                        ui.label(egui::RichText::new("Entities:").small().strong());

                                        for (entity_idx, entity) in analysis
                                            .ner_analysis
                                            .entities
                                            .iter()
                                            .take(3)
                                            .enumerate()
                                        {
                                            ui.push_id(
                                                format!("entity_{idx}_{entity_idx}"),
                                                |ui| {
                                                    ui.horizontal(|ui| {
                                                        let entity_color = match entity
                                                            .label
                                                            .as_str()
                                                        {
                                                            "person" => egui::Color32::from_rgb(
                                                                100, 149, 237,
                                                            ),
                                                            "email" => {
                                                                egui::Color32::from_rgb(220, 20, 60)
                                                            }
                                                            "phone" => {
                                                                egui::Color32::from_rgb(255, 69, 0)
                                                            }
                                                            "organisation" => {
                                                                egui::Color32::from_rgb(
                                                                    138, 43, 226,
                                                                )
                                                            }
                                                            "location" => {
                                                                egui::Color32::from_rgb(34, 139, 34)
                                                            }
                                                            _ => egui::Color32::GRAY,
                                                        };

                                                        ui.label(
                                                            egui::RichText::new(format!(
                                                                "[{}]",
                                                                entity.label
                                                            ))
                                                            .small()
                                                            .color(entity_color)
                                                            .strong(),
                                                        );
                                                        ui.label(
                                                            egui::RichText::new(&entity.text)
                                                                .small()
                                                                .monospace()
                                                                .color(egui::Color32::DARK_BLUE),
                                                        );
                                                        ui.with_layout(
                                                            egui::Layout::right_to_left(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                ui.label(
                                                                    egui::RichText::new(format!(
                                                                        "{:.2}",
                                                                        entity.risk_score
                                                                    ))
                                                                    .small()
                                                                    .color(egui::Color32::RED),
                                                                );
                                                            },
                                                        );
                                                    });
                                                },
                                            );
                                        }

                                        if analysis.ner_analysis.entities.len() > 3 {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "... and {} more entities",
                                                    analysis.ner_analysis.entities.len() - 3
                                                ))
                                                .small()
                                                .color(egui::Color32::GRAY),
                                            );
                                        }
                                    }

                                    if !analysis.syntactic_analysis.interesting_tokens.is_empty() {
                                        ui.add_space(4.0);
                                        ui.label(egui::RichText::new("Patterns:").small().strong());

                                        for (token_idx, (token, score)) in analysis
                                            .syntactic_analysis
                                            .interesting_tokens
                                            .iter()
                                            .take(3)
                                            .enumerate()
                                        {
                                            ui.push_id(format!("token_{idx}_{token_idx}"), |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(format!("• {token}"))
                                                            .small()
                                                            .monospace()
                                                            .color(egui::Color32::DARK_BLUE),
                                                    );
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.label(
                                                                egui::RichText::new(format!(
                                                                    "{score:.2}"
                                                                ))
                                                                .small()
                                                                .color(egui::Color32::BLUE),
                                                            );
                                                        },
                                                    );
                                                });
                                            });
                                        }

                                        if analysis.syntactic_analysis.interesting_tokens.len() > 3
                                        {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "... and {} more patterns",
                                                    analysis
                                                        .syntactic_analysis
                                                        .interesting_tokens
                                                        .len()
                                                        - 3
                                                ))
                                                .small()
                                                .color(egui::Color32::GRAY),
                                            );
                                        }
                                    }

                                    if analysis.requires_scribes_review {
                                        ui.add_space(4.0);
                                        ui.horizontal(|ui| {
                                            ui.label("⚠");
                                            ui.label(
                                                egui::RichText::new("Requires Scribe Review")
                                                    .small()
                                                    .color(egui::Color32::RED)
                                                    .strong(),
                                            );
                                        });
                                    }
                                });
                            });

                            ui.add_space(6.0);
                        }
                    } else {
                        ui.colored_label(Color32::RED, "Error loading insights");
                    }
                });
        });
    }

    fn render_message(&self, ui: &mut Ui, message: &TelegramMessage) {
        let timestamp = message.timestamp.format("%H:%M:%S").to_string();

        if message.is_outgoing {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                ui.group(|ui| {
                    ui.set_max_width(ui.available_width() * 0.7);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&message.text).color(Color32::WHITE));
                        ui.label(RichText::new(&timestamp).small().color(Color32::LIGHT_GRAY));
                    });
                });
            });
        } else {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                ui.group(|ui| {
                    ui.set_max_width(ui.available_width() * 0.7);
                    ui.vertical(|ui| {
                        if let Some(from) = &message.from_user {
                            ui.label(RichText::new(from).strong().color(Color32::LIGHT_BLUE));
                        }
                        ui.label(&message.text);
                        ui.label(RichText::new(&timestamp).small().color(Color32::GRAY));
                    });
                });
            });
        }

        ui.add_space(3.0);
    }
}
