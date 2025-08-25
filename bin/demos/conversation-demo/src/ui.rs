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



use crate::nlu_runtime::NluRuntime;
use crate::telegram_client::{TelegramClient, TelegramMessage};
use crate::Args;
use chrono::Utc;
use eframe::egui::{self, Color32, RichText, ScrollArea, TextEdit};
use sleet::execute_flow;
use sleet::flows::definition::{BlockDefinition, BlockType, FlowDefinition};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use steel::messaging::{
    Message as SteelMessage, MessageDestination, MessageMetadata, MetadataValue,
};
use stele::database::structured_store::StructuredStore;
use stele::nlu::orchestrator::data_models::KnowledgeNode;
use stele::nlu::LLMAdapter;
use stele::scribes::specialists::knowledge_scribe::enrich_utterance;
use surrealdb::RecordId;
use tokio::sync::mpsc;
use tracing::debug;

pub struct App {
    
    token: String,
    chat_id: String,
    connected: bool,
    status_message: String,
    discovered_chats: Vec<(i64, String)>,
    selected_chat_id: Option<i64>,
    message_input: String,

    messages: Arc<Mutex<Vec<TelegramMessage>>>,

    
    nlu_rt: Option<Arc<NluRuntime>>, 

    
    to_worker: Option<mpsc::UnboundedSender<WorkerMessage>>,
    from_worker: mpsc::UnboundedReceiver<AppMessage>,
    _from_worker_sender: mpsc::UnboundedSender<AppMessage>,

    runtime: tokio::runtime::Runtime,

    
    polling_running: bool,

    
    show_db_evolution: bool,
    show_kg_idioms: bool,

    
    db_summary: String,

    
    bitemporal_as_of: String,
    bitemporal_result: String,

    kg_examples: Vec<(String, String)>,
}

enum WorkerMessage {
    SendMessage(i64, String),
    DiscoverChats,
    RunOffload(String),
}

enum AppMessage {
    Connected,
    ConnectionFailed(String),
    ChatsDiscovered(Vec<(i64, String)>),
    NewMessages(Vec<TelegramMessage>),
    MessageSent,
    Error(String),
    DbChanged,
    OffloadDone(String),
}

#[derive(Debug, Clone, Copy)]
enum InputType {
    Question,
    Statement,
}

impl App {
    pub fn new(args: Args) -> Self {
        let (from_worker_sender, from_worker_receiver) = mpsc::unbounded_channel();
        Self {
            token: args.get_token().unwrap_or_default(),
            chat_id: args.get_chat_id().unwrap_or_default(),
            connected: false,
            status_message: "Enter bot token and connect".to_string(),
            discovered_chats: Vec::new(),
            selected_chat_id: args.get_chat_id().and_then(|s| s.parse().ok()),
            message_input: String::new(),
            messages: Arc::new(Mutex::new(Vec::new())),
            nlu_rt: None,
            to_worker: None,
            from_worker: from_worker_receiver,
            _from_worker_sender: from_worker_sender,
            runtime: tokio::runtime::Runtime::new().expect("tokio rt"),
            polling_running: false,
            show_db_evolution: true,
            show_kg_idioms: true,
            db_summary: String::new(),
            bitemporal_as_of: String::new(),
            bitemporal_result: String::new(),
            kg_examples: vec![
                (
                    "nodes->edge->nodes".into(),
                    "SELECT ->edge->nodes FROM nodes".into(),
                ),
                (
                    "projects->assigned->users".into(),
                    "SELECT ->assigned->users FROM projects".into(),
                ),
            ],
        }
    }

    fn connect(&mut self) {
        if self.token.trim().is_empty() {
            self.status_message = "Bot token cannot be empty".to_string();
            return;
        }
        if self.polling_running {
            self.status_message = "Already connected (polling active)".to_string();
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

        
        if self.nlu_rt.is_none() {
            let (nlu_tx, nlu_rx) = std::sync::mpsc::channel();
            let rt_sender = self._from_worker_sender.clone();
            self.runtime.spawn(async move {
                match NluRuntime::init().await {
                    Ok(rt) => {
                        let _ = nlu_tx.send(Arc::new(rt));
                        let _ = rt_sender.send(AppMessage::Connected);
                    }
                    Err(e) => {
                        let _ = rt_sender.send(AppMessage::Error(format!("NLU init failed: {e}")));
                    }
                }
            });
            
            if let Ok(rt) = nlu_rx.recv_timeout(std::time::Duration::from_millis(1500)) {
                self.nlu_rt = Some(rt);
            }
        }

        self.polling_running = true;
        self.runtime.spawn(async move {
            let mut client = TelegramClient::new(token);
            if let Err(e) = client.test_connection().await {
                let _ = from_worker
                    .send(AppMessage::ConnectionFailed(format!("Connection failed: {e}")));
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
                                    Ok(_) => { let _ = from_worker.send(AppMessage::MessageSent); },
                                    Err(e) => { let _ = from_worker.send(AppMessage::Error(format!("Failed to send: {e}"))); }
                                }
                            }
                            WorkerMessage::DiscoverChats => {
                                let discovered = client.discover_chats().await.unwrap_or_default();
                                let _ = from_worker.send(AppMessage::ChatsDiscovered(discovered));
                            }
                            WorkerMessage::RunOffload(payload) => {
                                
                                let mut flow = FlowDefinition::new("telegram_offload", "start");
                                flow.add_block(BlockDefinition::new(
                                    "start",
                                    BlockType::Compute {
                                        expression: "'ok'".into(),
                                        output_key: "result".into(),
                                        next_block: "end".into(),
                                    },
                                ));
                                flow.add_block(BlockDefinition::new("end", BlockType::Terminate));
                                let from_worker_c = from_worker.clone();
                                tokio::spawn(async move {
                                    let _ = execute_flow(flow, 10_000, None).await;
                                    let _ = from_worker_c.send(AppMessage::OffloadDone(payload));
                                });
                            }
                        }
                    },
                    _ = interval.tick() => {
                        match client.get_updates().await {
                            Ok(messages) => {
                                if !messages.is_empty() {
                                    let mut discovered = Vec::new();
                                    for msg in &messages {
                                        let name = msg.from_user.clone().unwrap_or_else(|| format!("Chat {}", msg.chat_id));
                                        discovered.push((msg.chat_id, name));
                                    }
                                    if !discovered.is_empty() { let _ = from_worker.send(AppMessage::ChatsDiscovered(discovered)); }
                                    let _ = from_worker.send(AppMessage::NewMessages(messages));
                                }
                            }
                            Err(e) => { debug!("Failed to get updates: {e}"); }
                        }
                    }
                }
            }
        });
    }

    fn send_message(&mut self) {
        if self.message_input.trim().is_empty() {
            return;
        }
        if let Some(chat_id) = self.selected_chat_id {
            if let Some(sender) = &self.to_worker {
                let text = self.message_input.clone();
                
                let outgoing = TelegramMessage {
                    message_id: 0,
                    from_user: Some("You".into()),
                    chat_id,
                    text: text.clone(),
                    timestamp: Utc::now(),
                    is_outgoing: true,
                };
                if let Ok(mut m) = self.messages.lock() {
                    m.push(outgoing);
                }
                let _ = sender.send(WorkerMessage::SendMessage(chat_id, text));
                self.message_input.clear();
            }
        } else {
            self.status_message = "Please select a chat first".into();
        }
    }

    fn handle_worker_messages(&mut self) {
        while let Ok(msg) = self.from_worker.try_recv() {
            match msg {
                AppMessage::Connected => {
                    self.connected = true;
                    self.status_message = "Connected".into();
                    
                    self.pull_db_summary();
                }
                AppMessage::ConnectionFailed(err) => {
                    self.connected = false;
                    self.status_message = err;
                }
                AppMessage::ChatsDiscovered(chats) => {
                    for (id, name) in chats {
                        if !self.discovered_chats.iter().any(|(eid, _)| *eid == id) {
                            self.discovered_chats.push((id, name));
                        }
                    }
                    if self.selected_chat_id.is_none() && !self.discovered_chats.is_empty() {
                        self.selected_chat_id = Some(self.discovered_chats[0].0);
                    }
                }
                AppMessage::NewMessages(new_msgs) => {
                    if let Ok(mut messages) = self.messages.lock() {
                        messages.retain(|m| m.message_id != 0);
                        messages.extend(new_msgs);
                        messages.sort_by_key(|m| m.timestamp);
                        messages.dedup_by_key(|m| m.message_id);
                        if messages.len() > 200 {
                            let len = messages.len();
                            messages.drain(0..len - 200);
                        }
                    }

                    
                    if let Some(rt) = &self.nlu_rt {
                        if let Some(chat_id) = self.selected_chat_id {
                            
                            if let Ok(messages) = self.messages.lock() {
                                if let Some(last_in) = messages
                                    .iter()
                                    .rev()
                                    .find(|m| !m.is_outgoing && m.chat_id == chat_id)
                                {
                                    let text = last_in.text.clone();
                                    let sender = last_in
                                        .from_user
                                        .clone()
                                        .unwrap_or_else(|| format!("chat:{}", last_in.chat_id));
                                    let dest =
                                        MessageDestination::Single(last_in.chat_id.to_string());
                                    let mut meta = MessageMetadata::new();
                                    meta.insert(
                                        "platform".into(),
                                        MetadataValue::String("telegram".into()),
                                    );
                                    meta.insert(
                                        "chat_id".into(),
                                        MetadataValue::String(last_in.chat_id.to_string()),
                                    );
                                    let steel_msg = SteelMessage::new(sender, dest, text.clone())
                                        .with_metadata(meta);
                                    let rt = rt.clone();
                                    let app_tx = self._from_worker_sender.clone();
                                    self.runtime.spawn(async move {
                                        match handle_incoming_message(rt, steel_msg).await {
                                            Ok(ProcessOutcome::StatementStored) => {
                                                let _ = app_tx.send(AppMessage::DbChanged);
                                            }
                                            Ok(ProcessOutcome::QuestionAnswer(ans)) => {
                                                let _ = validate_llm_answer(&ans);
                                            }
                                            Ok(ProcessOutcome::Error(e)) => {
                                                let _ = app_tx.send(AppMessage::Error(format!(
                                                    "Processing failed: {e}"
                                                )));
                                            }
                                            Err(e) => {
                                                let _ = app_tx.send(AppMessage::Error(format!(
                                                    "Processing failed: {e}"
                                                )));
                                            }
                                        }
                                    });
                                }
                            }
                        }
                    }
                }
                AppMessage::MessageSent => {}
                AppMessage::Error(err) => {
                    self.status_message = err;
                }
                AppMessage::DbChanged => {
                    
                    self.pull_db_summary();
                }
                AppMessage::OffloadDone(msg) => {
                    self.status_message = format!("Offload complete: {msg}");
                    
                    if let (Some(rt), Some(chat_id)) = (&self.nlu_rt, self.selected_chat_id) {
                        let rt = rt.clone();
                        let label = msg.clone();
                        let app_tx = self._from_worker_sender.clone();
                        self.runtime.spawn(async move {
                            
                            let store = if let Some(canon) = rt.canonical_db.clone() {
                                StructuredStore::new_with_clients(canon, rt.db.clone(), false)
                            } else {
                                StructuredStore::new(rt.db.clone())
                            };
                            
                            let details = serde_json::json!({"label": label});
                            match store.create_provenance_event("sleet_offload", details).await {
                                Ok(prov) => {
                                    
                                    let channel = format!("telegram:{chat_id}");
                                    let mut ids: Vec<String> = Vec::new();
                                    if let Ok(mut res) = rt
                                        .db
                                        .clone()
                                        .query("SELECT VALUE id FROM utterance WHERE from_source IN (SELECT VALUE id FROM source WHERE channel = $ch) ORDER BY created_at DESC LIMIT 50;")
                                        .bind(("ch", channel))
                                        .await
                                    {
                                        let vals: Vec<serde_json::Value> = res.take(0).unwrap_or_default();
                                        for v in vals {
                                            if let Some(s) = v.as_str() { ids.push(s.to_string()); }
                                        }
                                    }
                                    
                                    for uid in ids {
                                        if let Some((tb, idpart)) = uid.split_once(':') {
                                            let utt: RecordId = RecordId::from((tb, idpart));
                                            let _ = store.relate_utterance_to_provenance(&utt, &prov).await;
                                        }
                                    }
                                    
                                    let _ = app_tx.send(AppMessage::DbChanged);
                                }
                                Err(e) => {
                                    let _ = app_tx.send(AppMessage::Error(format!(
                                        "Failed to create provenance_event: {e}"
                                    )));
                                }
                            }
                        });
                    }
                }
            }
        }
    }

    fn render_messages(&self, ui: &mut egui::Ui) {
        if let Ok(messages) = self.messages.lock() {
            if messages.is_empty() {
                ui.colored_label(Color32::GRAY, "No messages yet. Send one!");
            } else {
                for msg in messages.iter() {
                    if Some(msg.chat_id) == self.selected_chat_id {
                        self.render_message(ui, msg);
                    }
                }
            }
        } else {
            ui.colored_label(Color32::RED, "Error loading messages");
        }
    }

    fn render_message(&self, ui: &mut egui::Ui, message: &TelegramMessage) {
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

    fn pull_db_summary(&mut self) {
        if self.nlu_rt.is_none() {
            return;
        }
        let rt = self.nlu_rt.as_ref().unwrap().clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.runtime.spawn(async move {
            let mut summary = String::new();
            if let Some(canon) = &rt.canonical_db {
                if let Ok(mut res) = canon.query("SELECT count() AS c FROM canonical_entity GROUP ALL; SELECT name, entity_type FROM canonical_entity LIMIT 5; SELECT title, start_at, location FROM canonical_event ORDER BY start_at DESC LIMIT 5;").await {
                    let counts: Vec<serde_json::Value> = res.take(0).unwrap_or_default();
                    let list: Vec<serde_json::Value> = res.take(1).unwrap_or_default();
                    let events: Vec<serde_json::Value> = res.take(2).unwrap_or_default();
                    let c = counts.first().and_then(|v| v.get("c")).and_then(|v| v.as_u64()).unwrap_or(0);
                    summary.push_str(&format!("Canonical entities: {c}\n"));
                    for item in list.iter() {
                        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("<unknown>");
                        let ty = item.get("entity_type").and_then(|v| v.as_str()).unwrap_or("<unknown>");
                        summary.push_str(&format!("- {name} [{ty}]\n"));
                    }
                    if !events.is_empty() {
                        summary.push_str("Recent events:\n");
                        for ev in events.iter() {
                            let title = ev.get("title").and_then(|v| v.as_str()).unwrap_or("<untitled>");
                            let when = ev.get("start_at").and_then(|v| v.as_str()).unwrap_or("?");
                            let loc = ev.get("location").and_then(|v| v.as_str()).unwrap_or("");
                            summary.push_str(&format!("  • {title} @ {when} {loc}\n"));
                        }
                    }
                } else {
                    summary.push_str("No canonical DB summary available.\n");
                }
            } else {
                summary.push_str("Canonical DB not configured.\n");
            }
            let _ = tx.send(summary);
        });
        if let Ok(s) = rx.recv_timeout(Duration::from_millis(1200)) {
            self.db_summary = s;
        }
    }

    fn pull_bitemporal_slice(&mut self) {
        if self.nlu_rt.is_none() {
            return;
        }
        let rt = self.nlu_rt.as_ref().unwrap().clone();
        let as_of = self.bitemporal_as_of.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.runtime.spawn(async move {
            let out = if let Some(canon) = &rt.canonical_db {
                let q = if as_of.trim().is_empty() {
                    "SELECT count() AS c FROM canonical_event GROUP ALL".to_string()
                } else {
                    "SELECT count() AS c FROM canonical_event WHERE start_at <= <datetime>"
                        .to_string()
                        + &as_of
                        + " GROUP ALL"
                };
                match canon.query(q).await {
                    Ok(mut r) => {
                        let v: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        let c = v
                            .first()
                            .and_then(|x| x.get("c"))
                            .and_then(|x| x.as_u64())
                            .unwrap_or(0);
                        format!(
                            "Events as-of {}: {c}",
                            if as_of.is_empty() {
                                "now".into()
                            } else {
                                as_of
                            }
                        )
                    }
                    Err(e) => format!("Slice failed: {e}"),
                }
            } else {
                "Canonical DB not configured.".into()
            };
            let _ = tx.send(out);
        });
        if let Ok(s) = rx.recv_timeout(Duration::from_millis(1500)) {
            self.bitemporal_result = s;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_worker_messages();

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Bot token:");
                ui.add(
                    TextEdit::singleline(&mut self.token)
                        .password(true)
                        .hint_text("<telegram bot token>"),
                );
                ui.label("Chat ID:");
                ui.add(TextEdit::singleline(&mut self.chat_id).hint_text("123456789"));
                if ui.button("Connect").clicked() {
                    self.connect();
                }
                if ui.button("Discover Chats").clicked() {
                    if let Some(sender) = &self.to_worker {
                        let _ = sender.send(WorkerMessage::DiscoverChats);
                    }
                }
                ui.separator();
                ui.label(
                    RichText::new(&self.status_message).color(if self.connected {
                        Color32::LIGHT_GREEN
                    } else {
                        Color32::YELLOW
                    }),
                );
            });
            if !self.discovered_chats.is_empty() {
                egui::ComboBox::from_label("Select chat")
                    .selected_text(
                        self.selected_chat_id
                            .map(|c| c.to_string())
                            .unwrap_or("<none>".into()),
                    )
                    .show_ui(ui, |ui| {
                        for (id, name) in &self.discovered_chats {
                            ui.selectable_value(
                                &mut self.selected_chat_id,
                                Some(*id),
                                format!("{name} ({id})"),
                            );
                        }
                    });
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let available_height = ui.available_height();
            ui.horizontal(|ui| {
                
                ui.allocate_ui_with_layout(
                    egui::Vec2::new(ui.available_width() * 0.50, available_height),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.heading("Messages");
                        ScrollArea::vertical()
                            .stick_to_bottom(true)
                            .max_height(available_height - 100.0)
                            .show(ui, |ui| {
                                self.render_messages(ui);
                            });
                        ui.separator();
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
                    },
                );

                
                ui.separator();
                ui.allocate_ui_with_layout(
                    egui::Vec2::new(ui.available_width(), available_height),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        if self.show_db_evolution {
                            ui.group(|ui| {
                                ui.heading("Database Evolution");
                                if ui.button("Refresh summary").clicked() {
                                    self.pull_db_summary();
                                }
                                ui.horizontal(|ui| {
                                    if ui.button("Run sleet offload").clicked() {
                                        if let Some(sender) = &self.to_worker {
                                            let _ = sender
                                                .send(WorkerMessage::RunOffload("echo".into()));
                                        }
                                    }
                                    ui.label(
                                        egui::RichText::new("demonstrates Phase 8 offload")
                                            .small()
                                            .italics(),
                                    );
                                });
                                if self.db_summary.is_empty() {
                                    ui.colored_label(Color32::GRAY, "No summary yet.");
                                } else {
                                    ui.code(self.db_summary.clone());
                                }
                                ui.add_space(6.0);
                                ui.collapsing("Bitemporal as-of slice", |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("as-of (ISO8601, blank = now):");
                                        ui.add(
                                            TextEdit::singleline(&mut self.bitemporal_as_of)
                                                .hint_text("2025-01-01T12:00:00Z"),
                                        );
                                        if ui.button("Run slice").clicked() {
                                            self.pull_bitemporal_slice();
                                        }
                                    });
                                    if self.bitemporal_result.is_empty() {
                                        ui.colored_label(Color32::GRAY, "No slice run yet.");
                                    } else {
                                        ui.code(self.bitemporal_result.clone());
                                    }
                                });
                            });
                        }
                        if self.show_kg_idioms {
                            ui.add_space(6.0);
                            ui.group(|ui| {
                                ui.heading("KG Idioms (examples)");
                                for (idiom, select) in &self.kg_examples {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(idiom).monospace());
                                        ui.label("→");
                                        ui.label(
                                            RichText::new(select)
                                                .small()
                                                .monospace()
                                                .color(Color32::LIGHT_BLUE),
                                        );
                                    });
                                }
                            });
                        }
                    },
                );
            });
        });
    }
}



#[derive(Debug)]
enum ProcessOutcome {
    StatementStored,
    QuestionAnswer(String),
    Error(String),
}

#[allow(dead_code)]
async fn process_text(
    rt: Arc<NluRuntime>,
    input: String,
) -> Result<ProcessOutcome, Box<dyn std::error::Error>> {
    
    let msg = SteelMessage::new(
        "telegram_user".into(),
        MessageDestination::Single("telegram_demo".into()),
        input,
    );
    handle_incoming_message(rt, msg).await
}

async fn determine_input_type(input: &str) -> Result<InputType, Box<dyn std::error::Error>> {
    
    Ok(simple_keyword_detection(input))
}

fn simple_keyword_detection(input: &str) -> InputType {
    let input_lower = input.to_lowercase();
    let q = [
        "find", "search", "show", "list", "get", "retrieve", "what", "where", "which", "who",
        "how many", "display", "fetch", "query", "look for", "tell me", "explain",
    ];
    if q.iter()
        .any(|k| input_lower.starts_with(k) || input_lower.starts_with(&format!("{k} ")))
    {
        InputType::Question
    } else {
        InputType::Statement
    }
}


async fn handle_incoming_message(
    rt: Arc<NluRuntime>,
    msg: SteelMessage,
) -> Result<ProcessOutcome, Box<dyn std::error::Error>> {
    let input = msg.content.clone();
    let user = msg.sender.clone();
    let channel = match &msg.destination {
        MessageDestination::Single(s) => format!("telegram:{s}"),
        MessageDestination::Multiple(v) => format!("telegram:{}", v.join(",")),
    };
    
    let store_val = match rt
        .query_processor
        .process_and_store_input(&input, &user, &channel)
        .await
    {
        Ok(v) => v,
        Err(e) => return Ok(ProcessOutcome::Error(format!("store failed: {e}"))),
    };
    
    match determine_input_type(&input)
        .await
        .unwrap_or(InputType::Statement)
    {
        InputType::Question => {
            let ans = handle_question(&input, &rt).await?;
            Ok(ProcessOutcome::QuestionAnswer(ans))
        }
        InputType::Statement => {
            
            if let Some(uid) = store_val.get("utterance_id").and_then(|x| x.as_str()) {
                let _ = enrich_utterance(rt.db.clone(), uid.to_string()).await;
            }
            Ok(ProcessOutcome::StatementStored)
        }
    }
}
async fn handle_question(
    query: &str,
    rt: &NluRuntime,
) -> Result<String, Box<dyn std::error::Error>> {
    match rt.access.query_natural_language(query).await {
        Ok(nodes) => {
            if nodes.is_empty() {
                return Ok(format!("No results found for: '{query}'"));
            }
            let node_ids: Vec<String> = nodes.iter().map(|n| n.temp_id().to_string()).collect();
            let utterances = rt
                .storage
                .get_utterances_for_nodes(&node_ids)
                .await
                .unwrap_or(serde_json::json!([]));
            let relevant_nodes = filter_relevant_nodes(&nodes, query);
            let prompt = create_focused_prompt(query, &relevant_nodes, &utterances);
            match rt.llm.generate_response(&prompt).await {
                Ok(ans) => Ok(ans),
                Err(_) => Ok(format!("Found {} result(s)", nodes.len())),
            }
        }
        Err(e) => Ok(format!("Error processing question: {e}")),
    }
}





fn filter_relevant_nodes<'a>(nodes: &'a [KnowledgeNode], query: &str) -> Vec<&'a KnowledgeNode> {
    let query_lower = query.to_lowercase();
    let mut relevant = Vec::new();
    let mut others = Vec::new();
    for node in nodes {
        let is_rel = match node {
            KnowledgeNode::Entity(e) => {
                query_lower.contains(&e.entity_type.to_lowercase())
                    || query_lower.contains(&e.name.to_lowercase())
                    || e.confidence > 0.9
            }
            KnowledgeNode::Action(a) => a.confidence > 0.8,
            KnowledgeNode::Temporal(_) | KnowledgeNode::Numerical(_) => false,
        };
        if is_rel {
            relevant.push(node);
        } else {
            others.push(node);
        }
    }
    if relevant.is_empty() {
        let mut s: Vec<_> = nodes.iter().collect();
        s.sort_by(|a, b| {
            let ca = match *a {
                KnowledgeNode::Entity(ref e) => e.confidence,
                KnowledgeNode::Action(ref a) => a.confidence,
                KnowledgeNode::Temporal(ref t) => t.confidence,
                KnowledgeNode::Numerical(ref n) => n.confidence,
            };
            let cb = match *b {
                KnowledgeNode::Entity(ref e) => e.confidence,
                KnowledgeNode::Action(ref a) => a.confidence,
                KnowledgeNode::Temporal(ref t) => t.confidence,
                KnowledgeNode::Numerical(ref n) => n.confidence,
            };
            cb.partial_cmp(&ca).unwrap_or(std::cmp::Ordering::Equal)
        });
        s.into_iter().take(10).collect()
    } else {
        relevant.extend(others.into_iter().take(5));
        relevant
    }
}

fn create_focused_prompt(
    query: &str,
    nodes: &[&KnowledgeNode],
    utterances: &serde_json::Value,
) -> String {
    let mut prompt = format!("You are a helpful assistant. The user asked: \"{query}\"\n\n");
    if let Some(arr) = utterances.as_array() {
        if !arr.is_empty() {
            prompt.push_str("Based on the following original statements:\n");
            for u in arr.iter().take(5) {
                if let Some(t) = u.get("raw_text").and_then(|t| t.as_str()) {
                    prompt.push_str(&format!("- \"{t}\"\n"));
                }
            }
            prompt.push('\n');
        }
    }
    if !nodes.is_empty() {
        prompt.push_str("I found the following relevant entities:\n");
        for n in nodes.iter().take(10) {
            match n {
                KnowledgeNode::Entity(e) => {
                    prompt.push_str(&format!(
                        "- {} ({}) : {:.0}% confidence\n",
                        e.name,
                        e.entity_type,
                        e.confidence * 100.0
                    ));
                }
                KnowledgeNode::Action(a) => {
                    prompt.push_str(&format!(
                        "- Action: {} ({:.0}% confidence)\n",
                        a.verb,
                        a.confidence * 100.0
                    ));
                }
                _ => {}
            }
        }
        prompt.push('\n');
    }
    prompt.push_str("Based on the provided data, give a helpful and concise answer to the user's question. Be conversational and focus on the most relevant information.");
    prompt
}

fn validate_llm_answer(ans: &str) -> bool {
    let s = ans.trim();
    if s.is_empty() || s.len() > 1000 {
        return false;
    }
    let lower = s.to_lowercase();
    if lower.contains("```")
        || lower.contains("<script")
        || lower.contains("system:")
        || lower.contains("assistant:")
    {
        return false;
    }
    true
}
