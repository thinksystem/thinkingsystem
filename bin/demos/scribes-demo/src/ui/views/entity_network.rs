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

use crate::ui::core::{EntityConnection, EntityNode, MessageParticle};
use egui::{Color32, Context, Pos2};
use std::collections::HashMap;

pub struct EntityNetworkWindow;

impl EntityNetworkWindow {
    pub fn new() -> Self {
        Self
    }

    pub fn show(
        &self,
        ctx: &Context,
        entities: &HashMap<String, EntityNode>,
        connections: &HashMap<String, EntityConnection>,
        particles: &[MessageParticle],
    ) {
        egui::Window::new("Entity Network")
            .id(egui::Id::new("entity_network_window"))
            .default_size([1000.0, 700.0])
            .show(ctx, |ui| {
                ui.label("Live Entity Network Visualisation");
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label(format!("Active entities: {}", entities.len()));

                    let active_count = entities.values().filter(|e| e.message_count > 0).count();
                    ui.label(format!("Active: {active_count}"));

                    let recent_count = entities
                        .values()
                        .filter(|e| {
                            let now = chrono::Utc::now();
                            now.signed_duration_since(e.last_activity).num_seconds() < 60
                        })
                        .count();
                    ui.label(format!("Recent activity: {recent_count}"));

                    ui.label(format!("Connections: {}", connections.len()));
                    ui.label(format!("Active particles: {}", particles.len()));
                });
                ui.separator();

                let network_height = (ui.available_height() - 200.0).max(100.0);
                ui.allocate_ui_with_layout(
                    egui::Vec2::new(ui.available_width(), network_height),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        Self::draw_network(ui, entities, connections, particles);
                    },
                );

                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("entity_network_details_scroll")
                    .max_height(150.0)
                    .show(ui, |ui| {
                        ui.label("Entity Details:");
                        for (id, entity) in entities.iter() {
                            ui.horizontal(|ui| {
                                let colour = if entity.message_count > 0 {
                                    egui::Color32::GREEN
                                } else {
                                    egui::Color32::GRAY
                                };

                                ui.colored_label(colour, "●");
                                ui.label(format!("{}: {} messages", id, entity.message_count));
                                ui.label(format!(
                                    "Last: {}",
                                    entity.last_activity.format("%H:%M:%S")
                                ));
                            });
                        }
                    });
            });
    }
    fn reset_node_positions(entity_nodes: &mut HashMap<String, EntityNode>) {
        let entity_count = entity_nodes.len();
        for (i, (_, node)) in entity_nodes.iter_mut().enumerate() {
            let angle = (i as f32) * 2.0 * std::f32::consts::PI / entity_count as f32;
            let radius = 200.0;
            node.position = Pos2::new(400.0 + radius * angle.cos(), 300.0 + radius * angle.sin());
            node.velocity = egui::Vec2::ZERO;
        }
    }

    fn draw_network(
        ui: &mut egui::Ui,
        entity_nodes: &HashMap<String, EntityNode>,
        entity_connections: &HashMap<String, EntityConnection>,
        message_particles: &[MessageParticle],
    ) {
        let available_rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(available_rect, egui::Sense::hover());

        if ui.is_rect_visible(available_rect) {
            let painter = ui.painter();

            Self::draw_connections(painter, available_rect, entity_nodes, entity_connections);

            Self::draw_particles(painter, available_rect, message_particles);

            Self::draw_entities(painter, available_rect, entity_nodes);
        }
    }

    fn draw_connections(
        painter: &egui::Painter,
        available_rect: egui::Rect,
        entity_nodes: &HashMap<String, EntityNode>,
        entity_connections: &HashMap<String, EntityConnection>,
    ) {
        for connection in entity_connections.values() {
            if let (Some(from_node), Some(to_node)) = (
                entity_nodes.get(&connection.from),
                entity_nodes.get(&connection.to),
            ) {
                let from_pos = Self::global_to_window_pos(from_node.position, available_rect);
                let to_pos = Self::global_to_window_pos(to_node.position, available_rect);

                let alpha = (connection.strength * 255.0) as u8;
                let line_color = Color32::from_rgba_unmultiplied(100, 150, 200, alpha);
                let stroke_width = 1.0 + connection.strength * 3.0;

                painter.line_segment(
                    [from_pos, to_pos],
                    egui::Stroke::new(stroke_width, line_color),
                );

                let direction = (to_pos - from_pos).normalized();
                let arrow_size = 8.0;
                let arrow_tip = to_pos - direction * (to_node.get_radius() + 2.0);
                let perpendicular = egui::Vec2::new(-direction.y, direction.x);

                let arrow_left =
                    arrow_tip - direction * arrow_size + perpendicular * arrow_size * 0.5;
                let arrow_right =
                    arrow_tip - direction * arrow_size - perpendicular * arrow_size * 0.5;

                painter.line_segment(
                    [arrow_tip, arrow_left],
                    egui::Stroke::new(stroke_width, line_color),
                );
                painter.line_segment(
                    [arrow_tip, arrow_right],
                    egui::Stroke::new(stroke_width, line_color),
                );
            }
        }
    }

    fn draw_particles(
        painter: &egui::Painter,
        available_rect: egui::Rect,
        message_particles: &[MessageParticle],
    ) {
        for particle in message_particles {
            let particle_pos = Self::global_to_window_pos(particle.position, available_rect);

            let size = 4.0;

            painter.rect_filled(
                egui::Rect::from_center_size(particle_pos, egui::Vec2::splat(size)),
                2.0,
                Color32::WHITE,
            );

            painter.circle_filled(
                particle_pos,
                size + 1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 30),
            );
        }
    }

    fn draw_entities(
        painter: &egui::Painter,
        available_rect: egui::Rect,
        entity_nodes: &HashMap<String, EntityNode>,
    ) {
        for (name, node) in entity_nodes {
            let centre = Self::global_to_window_pos(node.position, available_rect);
            let radius = node.get_radius();

            painter.circle_filled(
                centre + egui::Vec2::new(2.0, 2.0),
                radius,
                Color32::from_rgba_unmultiplied(0, 0, 0, 60),
            );
            painter.circle_filled(centre, radius, node.colour);
            painter.circle_stroke(centre, radius, egui::Stroke::new(2.0, Color32::WHITE));

            let time_since_activity = (chrono::Utc::now() - node.last_activity).num_seconds();
            if time_since_activity < 5 {
                let pulse_alpha = ((5 - time_since_activity) as f32 / 5.0 * 100.0) as u8;
                let pulse_radius = radius + 5.0 + time_since_activity as f32 * 2.0;
                painter.circle_stroke(
                    centre,
                    pulse_radius,
                    egui::Stroke::new(
                        2.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, pulse_alpha),
                    ),
                );
            }

            let text_pos = centre + egui::Vec2::new(0.0, radius + 10.0);
            painter.text(
                text_pos,
                egui::Align2::CENTER_TOP,
                format!("{}\n({})", name, node.message_count),
                egui::FontId::default(),
                Color32::WHITE,
            );
        }
    }

    fn global_to_window_pos(global_pos: Pos2, window_rect: egui::Rect) -> Pos2 {
        let normalised_x = global_pos.x / 800.0;
        let normalised_y = global_pos.y / 600.0;

        Pos2::new(
            window_rect.min.x + normalised_x * window_rect.width(),
            window_rect.min.y + normalised_y * window_rect.height(),
        )
    }
}
