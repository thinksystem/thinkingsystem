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
use tokio::sync::mpsc;
use tracing::{debug, error};

pub struct TelegramApp {

    token: String,
    chat_id: String,
    connected: bool,
    status_message: String,


    discovered_chats: Vec<(i64, String)>,
    selected_chat_id: Option<i64>,


    message_input: String,


    messages: Arc<Mutex<Vec<TelegramMessage>>>,


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

        Self {
            token: args.get_token().unwrap_or_default(),
            chat_id: args.get_chat_id().unwrap_or_default(),
            connected: false,
            status_message: "Enter bot token and connect to discover chats".to_string(),
            discovered_chats: Vec::new(),
            selected_chat_id: args.get_chat_id().and_then(|s| s.parse().ok()),
            message_input: String::new(),
            messages: Arc::new(Mutex::new(Vec::new())),
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
            self.discovered_chats.push((chat_id, format!("Chat {chat_id} (from config)")));
            self.selected_chat_id = Some(chat_id);
        }

        self.runtime.spawn(async move {
            let mut client = TelegramClient::new(token);


            if let Err(e) = client.test_connection().await {
                let _ = from_worker.send(AppMessage::ConnectionFailed(format!("Connection failed: {e}")));
                return;
            }

            let _ = from_worker.send(AppMessage::Connected);


            let discovered = Self::discover_chats_from_updates(&mut client).await;
            let _ = from_worker.send(AppMessage::ChatsDiscovered(discovered));

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
                        if !self.discovered_chats.iter().any(|(existing_id, _)| *existing_id == id) {
                            self.discovered_chats.push((id, name));
                        }
                    }


                    if self.selected_chat_id.is_none() && !self.discovered_chats.is_empty() {
                        self.selected_chat_id = Some(self.discovered_chats[0].0);
                    }

                    if self.discovered_chats.is_empty() {
                        self.status_message = "No chats found. Send a message to your bot first, then try reconnecting.".to_string();
                    } else {
                        self.status_message = format!("Ready! Found {} chat(s)", self.discovered_chats.len());
                    }
                }
                AppMessage::NewMessages(new_msgs) => {
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
                }
                AppMessage::MessageSent => {

                }
                AppMessage::Error(err) => {
                    self.status_message = err;
                }
            }
        }
    }
}

impl eframe::App for TelegramApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.handle_worker_messages();
        ctx.request_repaint_after(Duration::from_millis(500));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Telegram Demo with Auto-Discovery");
            ui.separator();


            ui.group(|ui| {
                ui.heading("Connection");

                ui.horizontal(|ui| {
                    ui.label("Bot Token:");
                    ui.add(TextEdit::singleline(&mut self.token)
                        .password(true)
                        .hint_text("Enter your bot token"));
                });

                ui.horizontal(|ui| {
                    ui.label("Chat ID (optional):");
                    ui.add(TextEdit::singleline(&mut self.chat_id)
                        .hint_text("Fallback chat ID"));
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
                        let selected_text = self.selected_chat_id
                            .and_then(|id| self.discovered_chats.iter().find(|(chat_id, _)| *chat_id == id))
                            .map(|(_, name)| name.clone())
                            .unwrap_or_else(|| "Select a chat...".to_string());

                        ComboBox::from_label("")
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                for (chat_id, name) in &self.discovered_chats {
                                    ui.selectable_value(&mut self.selected_chat_id, Some(*chat_id), name);
                                }
                            });
                    });
                }
            });

            ui.separator();

            if self.connected && self.selected_chat_id.is_some() {

                let available_height = ui.available_height();
                ui.vertical(|ui| {
                    ui.group(|ui| {
                        ScrollArea::vertical()
                            .stick_to_bottom(true)
                            .max_height(available_height - 80.0)
                            .show(ui, |ui| {
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
                            });
                    });


                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            let response = ui.add(
                                TextEdit::singleline(&mut self.message_input)
                                    .hint_text("Type your message...")
                                    .desired_width(ui.available_width() - 70.0)
                            );

                            if ui.button("Send").clicked() ||
                               (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                                self.send_message();
                            }
                        });
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
