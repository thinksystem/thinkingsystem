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

use crate::scenario_generator::InteractiveScenarioGenerator;
use crate::ui::views::{
    EntityNetworkWindow, LLMMonitorWindow, LearningSystemWindow, ScribeWindow, StartupWindow,
};
use egui::{Color32, Context, Pos2, TextEdit};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct ScribeMessage {
    pub scribe_name: String,
    pub operation: String,
    pub input: Value,
    pub output: Option<Value>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub processing_time_ms: Option<u128>,
    pub is_llm_call: bool,
}

#[derive(Debug, Clone)]
pub struct LLMInteraction {
    pub scribe_name: String,
    pub prompt: String,
    pub response: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub provider: String,
    pub tokens_used: Option<u32>,
    pub cost: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct EntityNode {
    pub name: String,
    pub message_count: u32,
    pub colour: Color32,
    pub position: Pos2,
    pub velocity: egui::Vec2,
    pub last_activity: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct MessageParticle {
    pub from_entity: String,
    pub to_entity: String,
    pub position: Pos2,
    pub progress: f32,
    pub created_time: chrono::DateTime<chrono::Utc>,
    pub message_type: String,
}

#[derive(Debug, Clone)]
pub struct EntityConnection {
    pub from: String,
    pub to: String,
    pub strength: f32,
    pub last_message: chrono::DateTime<chrono::Utc>,
}

impl EntityNode {
    pub fn new(name: String) -> Self {
        let hash = name
            .chars()
            .fold(0u32, |acc, c| acc.wrapping_mul(31).wrapping_add(c as u32));
        let colour = Color32::from_rgb(
            ((hash & 0xFF0000) >> 16) as u8,
            ((hash & 0x00FF00) >> 8) as u8,
            (hash & 0x0000FF) as u8,
        );

        let position = Pos2::new(
            (hash % 400) as f32 + 100.0,
            ((hash >> 8) % 300) as f32 + 100.0,
        );

        Self {
            name,
            message_count: 0,
            colour,
            position,
            velocity: egui::Vec2::ZERO,
            last_activity: chrono::Utc::now(),
        }
    }

    pub fn increment_activity(&mut self) {
        self.message_count += 1;
        self.last_activity = chrono::Utc::now();
    }

    pub fn get_radius(&self) -> f32 {
        let base_radius = 15.0;

        let growth = if self.message_count <= 10 {
            self.message_count as f32 * 0.1
        } else if self.message_count <= 110 {
            1.0 + (self.message_count - 10) as f32 * 0.01
        } else {
            2.0 + (self.message_count - 110) as f32 * 0.001
        };

        base_radius * (1.0 + growth).min(5.0)
    }
}

pub struct ScribeUI {
    scribe_messages: Arc<Mutex<VecDeque<ScribeMessage>>>,
    llm_interactions: Arc<Mutex<VecDeque<LLMInteraction>>>,
    message_receiver: Arc<Mutex<mpsc::UnboundedReceiver<ScribeMessage>>>,
    llm_receiver: Arc<Mutex<mpsc::UnboundedReceiver<LLMInteraction>>>,

    entity_nodes: HashMap<String, EntityNode>,
    entity_connections: HashMap<String, EntityConnection>,
    message_particles: Vec<MessageParticle>,
    last_physics_update: chrono::DateTime<chrono::Utc>,

    knowledge_window: ScribeWindow,
    data_window: ScribeWindow,
    identity_window: ScribeWindow,
    learning_window: LearningSystemWindow,
    llm_monitor: LLMMonitorWindow,
    entity_network: EntityNetworkWindow,
    startup_window: StartupWindow,

    show_knowledge_scribe: bool,
    show_data_scribe: bool,
    show_identity_scribe: bool,
    show_learning_system: bool,
    show_llm_monitor: bool,
    show_entity_network: bool,
    show_startup: bool,

    filter_scribe: String,
    auto_scroll: bool,
    max_messages: usize,

    rt: Handle,
    scenario_generator: Arc<InteractiveScenarioGenerator>,

    selected_scenario_data: Option<String>,
    demo_should_start: bool,
    demo_started: bool,
}

impl ScribeUI {
    pub fn new(
        message_receiver: mpsc::UnboundedReceiver<ScribeMessage>,
        llm_receiver: mpsc::UnboundedReceiver<LLMInteraction>,
        rt: Handle,
        scenario_generator: Arc<InteractiveScenarioGenerator>,
    ) -> Self {
        Self {
            scribe_messages: Arc::new(Mutex::new(VecDeque::new())),
            llm_interactions: Arc::new(Mutex::new(VecDeque::new())),
            message_receiver: Arc::new(Mutex::new(message_receiver)),
            llm_receiver: Arc::new(Mutex::new(llm_receiver)),

            entity_nodes: HashMap::new(),
            entity_connections: HashMap::new(),
            message_particles: Vec::new(),
            last_physics_update: chrono::Utc::now(),

            knowledge_window: ScribeWindow::new(
                "Knowledge Scribe".to_string(),
                "Knowledge".to_string(),
            ),
            data_window: ScribeWindow::new("Data Scribe".to_string(), "Data".to_string()),
            identity_window: ScribeWindow::new(
                "Identity Scribe".to_string(),
                "Identity".to_string(),
            ),
            learning_window: LearningSystemWindow::new(),
            llm_monitor: LLMMonitorWindow::new(),
            entity_network: EntityNetworkWindow::new(),
            startup_window: StartupWindow::new(rt.clone()),

            show_knowledge_scribe: true,
            show_data_scribe: true,
            show_identity_scribe: true,
            show_learning_system: true,
            show_llm_monitor: true,
            show_entity_network: true,
            show_startup: true,

            filter_scribe: String::new(),
            auto_scroll: true,
            max_messages: 1000,
            rt,
            scenario_generator,

            selected_scenario_data: None,
            demo_should_start: false,
            demo_started: false,
        }
    }

    pub fn update(&mut self, ctx: &Context) {
        self.poll_messages();

        self.update_physics();

        self.update_message_particles();

        if !self.message_particles.is_empty() || !self.entity_connections.is_empty() {
            ctx.request_repaint();
        }

        if !self.show_startup {
            self.show_menu_bar(ctx);
        }

        self.show_windows(ctx);
    }

    pub fn should_start_demo(&mut self) -> Option<Option<String>> {
        if self.demo_should_start && !self.demo_started {
            self.demo_started = true;
            Some(self.selected_scenario_data.clone())
        } else {
            None
        }
    }

    fn show_menu_bar(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("Windows", |ui| {
                    ui.add(egui::Checkbox::new(
                        &mut self.show_knowledge_scribe,
                        "Knowledge Scribe",
                    ));
                    ui.add(egui::Checkbox::new(
                        &mut self.show_data_scribe,
                        "Data Scribe",
                    ));
                    ui.add(egui::Checkbox::new(
                        &mut self.show_identity_scribe,
                        "Identity Scribe",
                    ));
                    ui.add(egui::Checkbox::new(
                        &mut self.show_learning_system,
                        "Learning System",
                    ));
                    ui.add(egui::Checkbox::new(
                        &mut self.show_llm_monitor,
                        "LLM Monitor",
                    ));
                    ui.add(egui::Checkbox::new(
                        &mut self.show_entity_network,
                        "Entity Network",
                    ));
                });

                ui.separator();

                ui.label("Filter:");
                ui.add(
                    TextEdit::singleline(&mut self.filter_scribe)
                        .id(egui::Id::new("global_filter_input"))
                        .hint_text("Filter messages..."),
                );

                ui.separator();

                ui.add(egui::Checkbox::new(&mut self.auto_scroll, "Auto-scroll"));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Ok(messages) = self.scribe_messages.try_lock() {
                        ui.label(format!("Messages: {}", messages.len()));
                    }
                    if let Ok(interactions) = self.llm_interactions.try_lock() {
                        ui.label(format!("LLM: {}", interactions.len()));
                    }
                    ui.label(format!("Entities: {}", self.entity_nodes.len()));
                });
            });
        });
    }

    fn show_windows(&mut self, ctx: &Context) {
        if self.show_startup {
            let selected_scenarios =
                self.startup_window
                    .show(ctx, &mut self.show_startup, &self.scenario_generator);

            if let Some(scenarios_text) = selected_scenarios {
                self.show_startup = false;
                self.demo_should_start = true;
                self.selected_scenario_data = Some(scenarios_text);
                tracing::info!("Using all generated scenarios for demo execution");
            } else if !self.show_startup {
                self.demo_should_start = true;
                self.selected_scenario_data = None;
            } else {
                return;
            }
        }

        if self.show_knowledge_scribe {
            self.knowledge_window
                .show(ctx, &self.scribe_messages, "knowledge_scribe_window");
        }

        if self.show_data_scribe {
            self.data_window
                .show(ctx, &self.scribe_messages, "data_scribe_window");
        }

        if self.show_identity_scribe {
            self.identity_window
                .show(ctx, &self.scribe_messages, "identity_scribe_window");
        }

        if self.show_learning_system {
            self.learning_window.show(ctx, &self.scribe_messages);
        }

        if self.show_llm_monitor {
            self.llm_monitor.show(ctx, &self.llm_interactions);
        }

        if self.show_entity_network {
            self.entity_network.show(
                ctx,
                &self.entity_nodes,
                &self.entity_connections,
                &self.message_particles,
            );
        }
    }

    fn poll_messages(&mut self) {
        let mut new_scribe_messages = Vec::new();
        if let Ok(mut receiver) = self.message_receiver.try_lock() {
            while let Ok(message) = receiver.try_recv() {
                println!(
                    "📨 ScribeUI: Received message for {} - {}",
                    message.scribe_name, message.operation
                );
                new_scribe_messages.push(message);
            }
        } else {
            println!("❌ ScribeUI: Failed to lock message receiver");
        }

        for message in new_scribe_messages {
            self.update_entity_activity(&message);

            if let Ok(mut messages) = self.scribe_messages.try_lock() {
                messages.push_back(message);
                if messages.len() > self.max_messages {
                    messages.pop_front();
                }
                println!("ScribeUI: Total messages in queue: {}", messages.len());
            } else {
                println!("❌ ScribeUI: Failed to lock messages collection");
            }
        }

        let mut new_llm_interactions = Vec::new();
        if let Ok(mut receiver) = self.llm_receiver.try_lock() {
            while let Ok(interaction) = receiver.try_recv() {
                println!(
                    "ScribeUI: Received LLM interaction for {} - {}",
                    interaction.scribe_name, interaction.provider
                );
                new_llm_interactions.push(interaction);
            }
        } else {
            println!("❌ ScribeUI: Failed to lock LLM receiver");
        }

        for interaction in new_llm_interactions {
            let llm_entity = format!("LLM-{}", interaction.provider);
            let entry = self
                .entity_nodes
                .entry(llm_entity.clone())
                .or_insert_with(|| EntityNode::new(format!("LLM-{}", interaction.provider)));
            entry.increment_activity();

            let entry = self
                .entity_nodes
                .entry(interaction.scribe_name.clone())
                .or_insert_with(|| EntityNode::new(interaction.scribe_name.clone()));
            entry.increment_activity();

            self.create_connection(&interaction.scribe_name, &llm_entity, "llm_call");
            self.create_message_particle(&interaction.scribe_name, &llm_entity, "llm_call");

            self.create_connection(&llm_entity, &interaction.scribe_name, "llm_response");
            self.create_message_particle(&llm_entity, &interaction.scribe_name, "llm_response");

            if let Ok(mut interactions) = self.llm_interactions.try_lock() {
                interactions.push_back(interaction);
                if interactions.len() > self.max_messages {
                    interactions.pop_front();
                }
                println!(
                    "ScribeUI: Total LLM interactions in queue: {}",
                    interactions.len()
                );
            } else {
                println!("❌ ScribeUI: Failed to lock LLM interactions collection");
            }
        }
    }

    fn update_entity_activity(&mut self, message: &ScribeMessage) {
        let scribe_entity = &message.scribe_name;
        let entry = self
            .entity_nodes
            .entry(scribe_entity.clone())
            .or_insert_with(|| EntityNode::new(scribe_entity.clone()));
        entry.increment_activity();

        let mut target_entities = Vec::new();

        self.extract_and_track_system_entities(&message.input, &mut target_entities);
        if let Some(output) = &message.output {
            self.extract_and_track_system_entities(output, &mut target_entities);
        }

        if message.is_llm_call {
            if let Some(provider) = self.extract_llm_provider(&message.input, &message.output) {
                let llm_entity = format!("LLM-{provider}");
                let entry = self
                    .entity_nodes
                    .entry(llm_entity.clone())
                    .or_insert_with(|| EntityNode::new(format!("LLM-{provider}")));
                entry.increment_activity();
                target_entities.push(llm_entity);
            }
        }

        let operation_entities =
            self.get_operation_target_entities(&message.operation, &message.scribe_name);
        for entity in operation_entities {
            if !target_entities.contains(&entity) {
                let entry = self
                    .entity_nodes
                    .entry(entity.clone())
                    .or_insert_with(|| EntityNode::new(entity.clone()));
                entry.increment_activity();
                target_entities.push(entity);
            }
        }

        for target_entity in target_entities {
            self.create_connection(scribe_entity, &target_entity, &message.operation);
            self.create_message_particle(scribe_entity, &target_entity, &message.operation);
        }
    }

    fn update_physics(&mut self) {
        let now = chrono::Utc::now();
        let dt = (now - self.last_physics_update).num_milliseconds() as f32 / 1000.0;
        self.last_physics_update = now;

        if dt > 0.1 || dt <= 0.0 {
            return;
        }

        let node_names: Vec<String> = self.entity_nodes.keys().cloned().collect();

        for i in 0..node_names.len() {
            let mut force = egui::Vec2::ZERO;
            let node_i = &self.entity_nodes[&node_names[i]];

            for j in 0..node_names.len() {
                if i != j {
                    let node_j = &self.entity_nodes[&node_names[j]];
                    let delta = node_i.position - node_j.position;
                    let distance = delta.length().max(15.0);

                    if distance < 500.0 {
                        let repulsion_strength = 12000.0 / (distance * distance);
                        force += delta.normalized() * repulsion_strength;
                    }
                }
            }

            for connection in self.entity_connections.values() {
                if connection.from == node_names[i] {
                    if let Some(target_node) = self.entity_nodes.get(&connection.to) {
                        let delta = target_node.position - node_i.position;
                        let distance = delta.length();
                        let desired_distance = 180.0;

                        let spring_constant = connection.strength * 100.0;
                        let displacement = distance - desired_distance;
                        if displacement.abs() > 10.0 {
                            force += delta.normalized() * spring_constant * displacement;
                        }
                    }
                }
                if connection.to == node_names[i] {
                    if let Some(source_node) = self.entity_nodes.get(&connection.from) {
                        let delta = source_node.position - node_i.position;
                        let distance = delta.length();
                        let desired_distance = 180.0;

                        let spring_constant = connection.strength * 100.0;
                        let displacement = distance - desired_distance;
                        if displacement.abs() > 10.0 {
                            force += delta.normalized() * spring_constant * displacement;
                        }
                    }
                }
            }

            let centre = Pos2::new(400.0, 300.0);
            let to_center = centre - node_i.position;
            let center_distance = to_center.length();
            if center_distance > 320.0 {
                let center_force = (center_distance - 320.0) * 0.15;
                force += to_center.normalized() * center_force;
            }

            if let Some(node) = self.entity_nodes.get_mut(&node_names[i]) {
                node.velocity += force * dt;

                let velocity_mag = node.velocity.length();
                let damping = if velocity_mag > 50.0 {
                    0.85
                } else if velocity_mag < 1.0 {
                    0.98
                } else {
                    0.92
                };
                node.velocity *= damping;

                if node.velocity.length() < 0.5 {
                    node.velocity = egui::Vec2::ZERO;
                }

                let max_velocity = 80.0;
                if node.velocity.length() > max_velocity {
                    node.velocity = node.velocity.normalized() * max_velocity;
                }

                node.position += node.velocity * dt;

                let margin = 20.0;

                if node.position.x < margin {
                    node.position.x = margin;
                    node.velocity.x = node.velocity.x.max(0.0);
                } else if node.position.x > 780.0 - margin {
                    node.position.x = 780.0 - margin;
                    node.velocity.x = node.velocity.x.min(0.0);
                }

                if node.position.y < margin {
                    node.position.y = margin;
                    node.velocity.y = node.velocity.y.max(0.0);
                } else if node.position.y > 580.0 - margin {
                    node.position.y = 580.0 - margin;
                    node.velocity.y = node.velocity.y.min(0.0);
                }
            }
        }

        let cutoff_time = now - chrono::Duration::seconds(30);
        self.entity_connections.retain(|_, connection| {
            connection.strength *= 0.995;
            connection.last_message > cutoff_time && connection.strength > 0.1
        });
    }

    fn update_message_particles(&mut self) {
        let now = chrono::Utc::now();

        for particle in &mut self.message_particles {
            let age_seconds = (now - particle.created_time).num_milliseconds() as f32 / 1000.0;
            particle.progress = (age_seconds * 0.8).min(1.0);

            let from_pos = self
                .entity_nodes
                .get(&particle.from_entity)
                .map(|n| n.position)
                .unwrap_or_else(|| {
                    let new_node = EntityNode::new(particle.from_entity.clone());
                    let pos = new_node.position;
                    self.entity_nodes
                        .insert(particle.from_entity.clone(), new_node);
                    pos
                });

            let to_pos = self
                .entity_nodes
                .get(&particle.to_entity)
                .map(|n| n.position)
                .unwrap_or_else(|| {
                    let new_node = EntityNode::new(particle.to_entity.clone());
                    let pos = new_node.position;
                    self.entity_nodes
                        .insert(particle.to_entity.clone(), new_node);
                    pos
                });

            particle.position = from_pos.lerp(to_pos, particle.progress);
        }

        self.message_particles
            .retain(|particle| particle.progress < 1.0);
    }

    fn extract_llm_provider(&self, input: &Value, output: &Option<Value>) -> Option<String> {
        if let Value::Object(obj) = input {
            if let Some(provider) = obj.get("provider").and_then(|v| v.as_str()) {
                return Some(provider.to_string());
            }
            if let Some(model) = obj.get("model").and_then(|v| v.as_str()) {
                if model.contains("claude") {
                    return Some("anthropic".to_string());
                } else if model.contains("gpt") {
                    return Some("openai".to_string());
                }
            }
        }

        if let Some(Value::Object(obj)) = output {
            if let Some(provider) = obj.get("provider").and_then(|v| v.as_str()) {
                return Some(provider.to_string());
            }
        }

        None
    }

    fn get_operation_target_entities(&self, operation: &str, scribe_name: &str) -> Vec<String> {
        let mut entities = Vec::new();

        match operation {
            "process_data" => {
                entities.push("LLM-anthropic".to_string());
                entities.push("Database".to_string());
            }
            "verify_source" | "link_identities" => {
                entities.push("IAM System".to_string());
                entities.push("Database".to_string());
            }
            "link_data_to_graph" => {
                entities.push("Database".to_string());
                entities.push("Knowledge Graph".to_string());
            }
            "store_extracted_data" => {
                entities.push("Database".to_string());
            }
            "choose_action" | "add_experience" | "update_q_values" => {
                entities.push("Q-Learning Engine".to_string());
            }
            _ => match scribe_name {
                "Data Scribe" => {
                    entities.push("Database".to_string());
                }
                "Identity Scribe" => {
                    entities.push("IAM System".to_string());
                }
                "Knowledge Scribe" => {
                    entities.push("Knowledge Graph".to_string());
                }
                "Q-Learning" => {
                    entities.push("Q-Learning Engine".to_string());
                }
                _ => {}
            },
        }

        entities
    }

    fn extract_and_track_system_entities(
        &mut self,
        value: &Value,
        target_entities: &mut Vec<String>,
    ) {
        match value {
            Value::Object(obj) => {
                for (key, v) in obj {
                    match key.as_str() {
                        "source_scribe" | "target_scribe" | "scribe_id" | "scribe_name" => {
                            if let Some(scribe_name) = v.as_str() {
                                let entry = self
                                    .entity_nodes
                                    .entry(scribe_name.to_string())
                                    .or_insert_with(|| EntityNode::new(scribe_name.to_string()));
                                entry.increment_activity();
                                target_entities.push(scribe_name.to_string());
                            }
                        }
                        "coordination_result" | "multi_specialist" => {
                            let coordinator_name = "Multi-Specialist Coordinator".to_string();
                            let entry = self
                                .entity_nodes
                                .entry(coordinator_name.clone())
                                .or_insert_with(|| EntityNode::new(coordinator_name.clone()));
                            entry.increment_activity();
                            target_entities.push(coordinator_name);
                        }
                        "learning_system" | "q_learning" | "learning_type" => {
                            let learning_name = "Learning System".to_string();
                            let entry = self
                                .entity_nodes
                                .entry(learning_name.clone())
                                .or_insert_with(|| EntityNode::new(learning_name.clone()));
                            entry.increment_activity();
                            target_entities.push(learning_name);
                        }
                        "database" | "storage" | "surreal" => {
                            let db_name = "Database".to_string();
                            let entry = self
                                .entity_nodes
                                .entry(db_name.clone())
                                .or_insert_with(|| EntityNode::new(db_name.clone()));
                            entry.increment_activity();
                            target_entities.push(db_name);
                        }
                        "iam" | "identity" | "authentication" => {
                            let iam_name = "IAM System".to_string();
                            let entry = self
                                .entity_nodes
                                .entry(iam_name.clone())
                                .or_insert_with(|| EntityNode::new(iam_name.clone()));
                            entry.increment_activity();
                            target_entities.push(iam_name);
                        }
                        _ => {}
                    }

                    self.extract_and_track_system_entities(v, target_entities);
                }
            }
            Value::Array(arr) => {
                for item in arr {
                    self.extract_and_track_system_entities(item, target_entities);
                }
            }
            _ => {}
        }
    }

    fn create_connection(&mut self, from: &str, to: &str, _message_type: &str) {
        let connection_key = format!("{from}→{to}");

        if let Some(connection) = self.entity_connections.get_mut(&connection_key) {
            connection.strength = (connection.strength + 0.1).min(1.0);
            connection.last_message = chrono::Utc::now();
        } else {
            self.entity_connections.insert(
                connection_key,
                EntityConnection {
                    from: from.to_string(),
                    to: to.to_string(),
                    strength: 0.3,
                    last_message: chrono::Utc::now(),
                },
            );
        }
    }

    fn create_message_particle(&mut self, from: &str, to: &str, message_type: &str) {
        if self.message_particles.len() < 100 {
            if !self.entity_nodes.contains_key(from) {
                self.entity_nodes
                    .insert(from.to_string(), EntityNode::new(from.to_string()));
            }
            if !self.entity_nodes.contains_key(to) {
                self.entity_nodes
                    .insert(to.to_string(), EntityNode::new(to.to_string()));
            }

            if let Some(from_node) = self.entity_nodes.get(from) {
                self.message_particles.push(MessageParticle {
                    from_entity: from.to_string(),
                    to_entity: to.to_string(),
                    position: from_node.position,
                    progress: 0.0,
                    created_time: chrono::Utc::now(),
                    message_type: message_type.to_string(),
                });

                println!("🔵 Created particle: {from} → {to} ({message_type})");
            } else {
                println!("⚠️ Failed to create particle: {from} → {to} (from entity missing)");
            }
        } else {
            println!(
                "⚠️ Particle limit reached: {} particles",
                self.message_particles.len()
            );
        }
    }
}

pub struct UIBridge {
    message_sender: mpsc::UnboundedSender<ScribeMessage>,
    llm_sender: mpsc::UnboundedSender<LLMInteraction>,
}

impl UIBridge {
    pub fn new() -> (
        Self,
        mpsc::UnboundedReceiver<ScribeMessage>,
        mpsc::UnboundedReceiver<LLMInteraction>,
    ) {
        let (message_tx, message_rx) = mpsc::unbounded_channel();
        let (llm_tx, llm_rx) = mpsc::unbounded_channel();

        (
            Self {
                message_sender: message_tx,
                llm_sender: llm_tx,
            },
            message_rx,
            llm_rx,
        )
    }

    pub fn log_scribe_operation(
        &self,
        scribe_name: &str,
        operation: &str,
        input: Value,
        output: Option<Value>,
        processing_time_ms: Option<u128>,
        is_llm_call: bool,
    ) {
        let message = ScribeMessage {
            scribe_name: scribe_name.to_string(),
            operation: operation.to_string(),
            input,
            output,
            timestamp: chrono::Utc::now(),
            processing_time_ms,
            is_llm_call,
        };

        let _ = self.message_sender.send(message);
    }

    pub fn log_llm_interaction(
        &self,
        scribe_name: &str,
        prompt: &str,
        response: &str,
        provider: &str,
        tokens_used: Option<u32>,
        cost: Option<f64>,
    ) {
        let interaction = LLMInteraction {
            scribe_name: scribe_name.to_string(),
            prompt: prompt.to_string(),
            response: response.to_string(),
            timestamp: chrono::Utc::now(),
            provider: provider.to_string(),
            tokens_used,
            cost,
        };

        let _ = self.llm_sender.send(interaction);
    }
}
