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


#![cfg(feature = "ui")]

use eframe::egui::{self, RichText, ScrollArea, StrokeKind, TextEdit};
use serde_json::Value;
use std::hash::{Hash, Hasher};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunMode {
    Auto,
    Native,
    Plan,
}

pub struct AppState {
    pub directive: String,
    pub logs: Vec<String>,
    pub running: bool,
    pub run_mode: RunMode,
    pub resolved_mode: Option<RunMode>,
    pub started_at: Option<Instant>,
    pub artifacts: Vec<String>,
    pub new_artifacts: Vec<String>,
    pub selected_artifact: Option<String>,
    pub artifact_preview: String,
    pub artifacts_dir: String,
    pub last_known_files: Vec<String>,
    pub last_artifact_scan: Option<Instant>,
    pub show_meta: bool,
    pub show_graph_window: bool,
    pub plan_nodes: Vec<GraphNode>,
    pub plan_edges: Vec<GraphEdge>,
    pub ir_nodes: Vec<GraphNode>,
    pub ir_edges: Vec<GraphEdge>,
    pub graph_last_file: Option<String>,
    pub graph_last_hash: Option<u64>,
    pub graph_zoom: f32,
    pub graph_mode: GraphMode,
    pub filter_plan: bool,
    pub filter_wat: bool,

    pub show_directives_modal: bool,
    pub directives: Vec<String>,
    pub directives_path: String,
    pub directives_query: String,
    pub selected_directive_index: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct GraphNode {
    pub id: String,
    pub kind: String,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct GraphEdge {
    pub from: usize,
    pub to: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphMode {
    Plan,
    IR,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            directive: "sum of squares 1 through 100".into(),
            logs: Vec::new(),
            running: false,
            run_mode: RunMode::Auto,
            resolved_mode: None,
            started_at: None,
            artifacts: Vec::new(),
            new_artifacts: Vec::new(),
            selected_artifact: None,
            artifact_preview: String::new(),
            artifacts_dir: "artifacts".into(),
            last_known_files: Vec::new(),
            last_artifact_scan: None,
            show_meta: false,
            show_graph_window: false,
            plan_nodes: Vec::new(),
            plan_edges: Vec::new(),
            ir_nodes: Vec::new(),
            ir_edges: Vec::new(),
            graph_last_file: None,
            graph_last_hash: None,
            graph_zoom: 1.0,
            graph_mode: GraphMode::Plan,
            filter_plan: true,
            filter_wat: true,
            show_directives_modal: false,
            directives: Vec::new(),
            directives_path: "bin/demos/flows-function-demo/directives.txt".into(),
            directives_query: String::new(),
            selected_directive_index: None,
        }
    }
}

impl eframe::App for AppState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("flows-function-demo UI").strong());
                if let Some(resolved) = self.resolved_mode {
                    ui.label(RichText::new(format!("Resolved: {resolved:?}")).monospace());
                }
                if let Some(start) = self.started_at {
                    ui.label(format!("Elapsed: {:.1}s", start.elapsed().as_secs_f32()));
                }
            });
        });
        egui::SidePanel::left("left_controls_panel")
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let run_kb;
                    {
                        let te = ui.add(
                            TextEdit::singleline(&mut self.directive)
                                .id_salt("directive_input")
                                .desired_width(260.0),
                        );
                        run_kb = te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    }
                    if ui
                        .button("Pick…")
                        .on_hover_text("Choose a directive from a list (⌘P)")
                        .clicked()
                    {
                        self.open_directives_modal();
                    }
                    if ui
                        .add_enabled(!self.running, egui::Button::new("Run"))
                        .on_hover_text("Execute the directive in selected or auto mode")
                        .clicked()
                        || run_kb
                    {
                        self.spawn(self.run_mode);
                    }
                    ui.separator();
                    for (label, mode) in [
                        ("Auto", RunMode::Auto),
                        ("Native", RunMode::Native),
                        ("Plan", RunMode::Plan),
                    ] {
                        ui.radio_value(&mut self.run_mode, mode, label);
                    }
                });
                if self.running {
                    ui.label(RichText::new("running...").italics());
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Artifacts dir:").monospace());
                    ui.add(
                        TextEdit::singleline(&mut self.artifacts_dir)
                            .id_salt("artifacts_dir_input")
                            .desired_width(180.0),
                    );
                });
                ui.small("Artifacts auto refresh every ~1s");
                ui.separator();
                ui.checkbox(&mut self.show_meta, "Show meta ROUTE/PATH lines");
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Filters:");
                    ui.checkbox(&mut self.filter_plan, "plan_*");
                    ui.checkbox(&mut self.filter_wat, "wat_*");
                });
            });
        egui::SidePanel::right("right_artifacts_panel")
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Artifacts");
                    if let Some(sel_plan) = self
                        .selected_artifact
                        .as_ref()
                        .filter(|f| f.starts_with("plan_"))
                    {
                        if ui
                            .add_enabled(!self.running, egui::Button::new("Re-run Plan"))
                            .on_hover_text("Execute selected plan JSON without LLM generation")
                            .clicked()
                        {
                            self.spawn_with_plan_file(sel_plan.clone());
                        }
                    }
                });
                let mut newly_selected: Option<String> = None;
                ScrollArea::vertical()
                    .id_salt("artifact_list_scroll")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        for file in &self.artifacts {
                            let mut lbl = file.clone();
                            if self.new_artifacts.contains(file) {
                                lbl.push_str(" *");
                            }
                            if ui
                                .selectable_label(
                                    self.selected_artifact.as_ref() == Some(file),
                                    lbl,
                                )
                                .clicked()
                            {
                                newly_selected = Some(file.clone());
                            }
                        }
                    });
                if let Some(sel) = newly_selected {
                    self.selected_artifact = Some(sel);
                    self.load_artifact_preview();
                }
                ui.separator();
                ui.label(RichText::new("Preview").strong());
                ScrollArea::vertical()
                    .id_salt("artifact_preview_scroll")
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.monospace(&self.artifact_preview);
                    });
                if let Some(plan_name) = self
                    .selected_artifact
                    .clone()
                    .filter(|s| s.starts_with("plan_"))
                {
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!self.running, egui::Button::new("Re-run This Plan"))
                            .clicked()
                        {
                            self.spawn_with_plan_file(plan_name.clone());
                        }
                        if ui
                            .button("Show Graph")
                            .on_hover_text("Open execution graph window")
                            .clicked()
                        {
                            self.show_graph_window = true;
                        }
                    });
                }
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical()
                .id_salt("logs_scroll")
                .auto_shrink([false; 2])
                .stick_to_bottom(self.running)
                .show(ui, |ui| {
                    for (idx, line) in self.logs.iter().enumerate() {
                        ui.push_id(idx, |ui| ui.monospace(line));
                    }
                });
        });

        if self.show_directives_modal {
            let mut keep_open = true;
            egui::Window::new("Select Directive")
                .collapsible(false)
                .resizable(true)
                .default_width(560.0)
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    if self.directives.is_empty() {
                        self.reload_directives();
                    }
                    ui.horizontal(|ui| {
                        ui.label("Filter:");
                        ui.add(
                            TextEdit::singleline(&mut self.directives_query)
                                .id_salt("directives_query")
                                .desired_width(360.0),
                        );
                        if ui.small_button("Reload").clicked() {
                            self.reload_directives();
                        }
                    });
                    ui.separator();
                    let query = self.directives_query.to_lowercase();
                    let filtered: Vec<(usize, &String)> = self
                        .directives
                        .iter()
                        .enumerate()
                        .filter(|(_, d)| query.is_empty() || d.to_lowercase().contains(&query))
                        .collect();
                    let mut chosen: Option<usize> = None;
                    ScrollArea::vertical()
                        .id_salt("directives_list")
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for (idx, d) in filtered {
                                ui.push_id(idx, |ui| {
                                    let selected = self.selected_directive_index == Some(idx);
                                    if ui.selectable_label(selected, d).clicked() {
                                        self.selected_directive_index = Some(idx);
                                    }
                                    if ui.small_button("Use").clicked() {
                                        chosen = Some(idx);
                                    }
                                });
                                ui.separator();
                            }
                        });
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.show_directives_modal = false;
                        }
                        let ok_enabled = self.selected_directive_index.is_some();
                        if ui
                            .add_enabled(ok_enabled, egui::Button::new("Use Selected"))
                            .clicked()
                        {
                            chosen = self.selected_directive_index;
                        }
                    });
                    if let Some(ch) = chosen {
                        if let Some(sel) = self.directives.get(ch) {
                            self.directive = sel.clone();
                        }
                        self.show_directives_modal = false;
                    }
                });
            if !keep_open {
                self.show_directives_modal = false;
            }
        }
        if self.show_graph_window {
            egui::Window::new("Execution Graph")
                .open(&mut self.show_graph_window)
                .resizable(true)
                .default_width(420.0)
                .default_height(300.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.graph_mode, GraphMode::Plan, "Plan");
                        ui.selectable_value(&mut self.graph_mode, GraphMode::IR, "IR Ops");
                        ui.separator();
                        ui.add(egui::Slider::new(&mut self.graph_zoom, 0.4..=2.0).text("zoom"));
                        if ui.small_button("Reset").clicked() {
                            self.graph_zoom = 1.0;
                        }
                        ui.label(match self.graph_mode {
                            GraphMode::Plan => format!("plan nodes: {}", self.plan_nodes.len()),
                            GraphMode::IR => format!("ir ops: {}", self.ir_nodes.len()),
                        });
                    });
                    let (nodes, edges) = match self.graph_mode {
                        GraphMode::Plan => (&self.plan_nodes, &self.plan_edges),
                        GraphMode::IR => (&self.ir_nodes, &self.ir_edges),
                    };
                    if nodes.is_empty() {
                        ui.label("No nodes parsed for this view.");
                        return;
                    }
                    ui.separator();
                    let zoom = self.graph_zoom;
                    const BASE_NODE_W: f32 = 160.0;
                    const BASE_NODE_H: f32 = 54.0;
                    const V_SPACING: f32 = 80.0;
                    const H_MARGIN: f32 = 40.0;
                    let node_w = BASE_NODE_W * zoom;
                    let node_h = BASE_NODE_H * zoom;
                    let v_spacing = V_SPACING * zoom;
                    let total_h = (nodes.len() as f32 * v_spacing + 40.0).max(160.0);
                    ScrollArea::vertical()
                        .id_salt("graph_scroll")
                        .show(ui, |ui| {
                            let avail_w = ui.available_width();
                            let (rect, _resp) = ui.allocate_exact_size(
                                egui::vec2(avail_w, total_h),
                                egui::Sense::hover(),
                            );
                            let painter = ui.painter_at(rect);
                            let lane_left = rect.left() + H_MARGIN;
                            let lane_right = rect.right() - (H_MARGIN + node_w);
                            let mut rects: Vec<egui::Rect> = Vec::with_capacity(nodes.len());
                            let mut y = rect.top() + 20.0;
                            for idx in 0..nodes.len() {
                                let is_left = idx % 2 == 0;
                                let x = if is_left { lane_left } else { lane_right };
                                let r = egui::Rect::from_min_size(
                                    egui::pos2(x, y),
                                    egui::vec2(node_w, node_h),
                                );
                                rects.push(r);
                                y += v_spacing;
                            }
                            for e in edges {
                                if let (Some(fr), Some(to)) = (rects.get(e.from), rects.get(e.to)) {
                                    let start = egui::pos2(fr.center().x, fr.bottom());
                                    let end = egui::pos2(to.center().x, to.top());
                                    painter.line_segment(
                                        [start, end],
                                        egui::Stroke::new(
                                            1.0,
                                            ui.visuals().widgets.noninteractive.fg_stroke.color,
                                        ),
                                    );
                                }
                            }
                            for (idx, node) in nodes.iter().enumerate() {
                                let node_rect = rects[idx];
                                painter.rect_filled(node_rect, 6.0, ui.visuals().extreme_bg_color);
                                painter.rect_stroke(
                                    node_rect,
                                    6.0,
                                    egui::Stroke::new(
                                        1.0,
                                        ui.visuals().widgets.noninteractive.fg_stroke.color,
                                    ),
                                    StrokeKind::Outside,
                                );
                                let text = format!("{}\n{}\n{}", node.kind, node.id, node.label);
                                painter.text(
                                    node_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    text,
                                    egui::FontId::proportional(11.0 * zoom.clamp(0.6, 1.4)),
                                    ui.visuals().text_color(),
                                );
                            }
                        });
                    ui.separator();
                    egui::CollapsingHeader::new("List")
                        .default_open(false)
                        .show(ui, |ui| {
                            for n in nodes {
                                ui.label(format!("{} | {} | {}", n.kind, n.id, n.label));
                            }
                        });
                });
        }
        LOG_RX.with(|r| {
            if let Some(rx) = &*r.borrow() {
                while let Ok(line) = rx.try_recv() {
                    if line == "REFRESH_ARTIFACTS" {
                        self.refresh_artifacts();
                        continue;
                    }
                    if line.starts_with("ROUTE requested=") {
                        if let Some(res_idx) = line.find(" resolved=") {
                            let mode_str =
                                line[res_idx + 10..].split_whitespace().next().unwrap_or("");
                            self.resolved_mode = match mode_str {
                                "Native" => Some(RunMode::Native),
                                "Plan" => Some(RunMode::Plan),
                                _ => self.resolved_mode,
                            };
                        }
                        if !self.show_meta {
                            continue;
                        }
                    }
                    if line.starts_with("PATH=") && !self.show_meta {
                        continue;
                    }
                    self.logs.push(line.clone());
                    if self.logs.len() > 4000 {
                        let excess = self.logs.len() - 4000;
                        self.logs.drain(0..excess);
                    }
                }
            }
        });
        if self.running && self.logs.iter().rev().any(|l| l.starts_with("DONE:")) {
            self.running = false;
        }
        let now = Instant::now();
        let needs_scan = match self.last_artifact_scan {
            Some(t) => now.duration_since(t).as_millis() > 1000,
            None => true,
        };
        if needs_scan {
            let prev = self.selected_artifact.clone();
            let prev_set: std::collections::HashSet<_> =
                self.last_known_files.iter().cloned().collect();
            let mut current = Self::list_artifact_files(&self.artifacts_dir);
            current.retain(|p| {
                let base = p.rsplit('/').next().unwrap_or(p);
                (self.filter_plan && base.starts_with("plan_"))
                    || (self.filter_wat && base.starts_with("wat_"))
                    || (!base.starts_with("plan_") && !base.starts_with("wat_"))
            });
            if current != self.last_known_files {
                let mut new = Vec::new();
                for f in &current {
                    if !prev_set.contains(f) {
                        new.push(f.clone());
                    }
                }
                self.artifacts = current.clone();
                self.new_artifacts = new;
                self.last_known_files = current;
                if let Some(sel) = &prev {
                    if !self.last_known_files.contains(sel) {
                        self.selected_artifact = None;
                    }
                }
                if self.selected_artifact.is_none() {
                    if let Some(last) = self.artifacts.last() {
                        self.selected_artifact = Some(last.clone());
                    }
                }
                if self.selected_artifact != prev {
                    self.load_artifact_preview();
                }
            }
            self.last_artifact_scan = Some(now);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(130));

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::P)) {
            self.open_directives_modal();
        }
    }
}

thread_local! { static LOG_RX: std::cell::RefCell<Option<Receiver<String>>> = const { std::cell::RefCell::new(None) }; }

impl AppState {
    fn open_directives_modal(&mut self) {
        self.show_directives_modal = true;
        self.selected_directive_index = None;
    }

    fn reload_directives(&mut self) {
        let path = &self.directives_path;
        let contents = std::fs::read_to_string(path).unwrap_or_else(|_| String::from(""));
        let mut list = Vec::new();
        for line in contents.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            list.push(t.to_string());
        }
        if list.is_empty() {
            list.push("sum of squares 1 through 100".into());
            list.push("sum of cubes 1 through 50".into());
        }
        self.directives = list;
    }

    pub fn spawn(&mut self, mode: RunMode) {
        self.running = true;
        self.logs.clear();
        self.resolved_mode = None;
        self.started_at = Some(Instant::now());
        self.new_artifacts.clear();
        self.last_known_files = Self::list_artifact_files(&self.artifacts_dir);
        let directive = self.directive.clone();
        let selected_mode = mode;
        let (log_tx, log_rx) = channel();
        LOG_RX.with(|cell| *cell.borrow_mut() = Some(log_rx));
        thread::spawn(move || {
            let cli_path = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("flows-function-demo")))
                .filter(|p| p.exists())
                .unwrap_or_else(|| {
                    let mut cand = std::path::PathBuf::from("target/debug/flows-function-demo");
                    if !cand.exists() {
                        cand = std::path::PathBuf::from("target/release/flows-function-demo");
                    }
                    cand
                });
            if !cli_path.exists() {
                let _ = log_tx.send(format!("ERROR: CLI binary not found at {cli_path:?}"));
                let _ = log_tx.send("DONE: \"err\"".into());
                return;
            }
            let mut cmd = std::process::Command::new(&cli_path);
            cmd.arg("--directive").arg(&directive);
            match selected_mode {
                RunMode::Native => {
                    cmd.arg("--llm-rust-fn");
                }
                RunMode::Plan => {
                    cmd.arg("--llm-plan");
                }
                RunMode::Auto => {}
            }
            cmd.envs(std::env::vars());
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
            let start_res = cmd.spawn();
            match start_res {
                Ok(mut child) => {
                    let stdout = child.stdout.take();
                    let stderr = child.stderr.take();
                    let tx_clone = log_tx.clone();
                    if let Some(out) = stdout {
                        let txo = log_tx.clone();
                        thread::spawn(move || {
                            use std::io::{BufRead, BufReader};
                            let reader = BufReader::new(out);
                            for line in reader.lines().map_while(|r| r.ok()) {
                                let _ = txo.send(line);
                            }
                        });
                    }
                    if let Some(err) = stderr {
                        thread::spawn(move || {
                            use std::io::{BufRead, BufReader};
                            let reader = BufReader::new(err);
                            for line in reader.lines().map_while(|r| r.ok()) {
                                let _ = tx_clone.send(line);
                            }
                        });
                    }
                    let status = child.wait();
                    let _ = log_tx.send(format!(
                        "DONE: {:?}",
                        status.ok().map(|s| if s.success() { "ok" } else { "fail" })
                    ));
                }
                Err(e) => {
                    let _ = log_tx.send(format!("ERROR: failed to spawn CLI: {e}"));
                    let _ = log_tx.send("DONE: \"err\"".into());
                }
            }
            let _ = log_tx.send("REFRESH_ARTIFACTS".into());
        });
    }

    pub fn spawn_with_plan_file(&mut self, plan_file: String) {
        self.running = true;
        self.logs.clear();
        self.resolved_mode = Some(RunMode::Plan);
        self.started_at = Some(Instant::now());
        self.new_artifacts.clear();
        self.last_known_files = Self::list_artifact_files(&self.artifacts_dir);

        let plan_path = match self.resolve_artifact_path(&plan_file) {
            Some(p) => p,
            None => {
                self.logs.push(format!(
                    "ERROR: plan file not found in '{}' or run_* subdirs: {}",
                    self.artifacts_dir, plan_file
                ));
                self.running = false;
                return;
            }
        };
        let (log_tx, log_rx) = channel();
        LOG_RX.with(|cell| *cell.borrow_mut() = Some(log_rx));
        thread::spawn(move || {
            let cli_path = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("flows-function-demo")))
                .filter(|p| p.exists())
                .unwrap_or_else(|| {
                    let mut cand = std::path::PathBuf::from("target/debug/flows-function-demo");
                    if !cand.exists() {
                        cand = std::path::PathBuf::from("target/release/flows-function-demo");
                    }
                    cand
                });
            if !cli_path.exists() {
                let _ = log_tx.send(format!("ERROR: CLI binary not found at {cli_path:?}"));
                let _ = log_tx.send("DONE: \"err\"".into());
                return;
            }
            let offline_supported = (|| {
                let out = std::process::Command::new(&cli_path)
                    .arg("--help")
                    .output()
                    .ok()?;
                let help = String::from_utf8_lossy(&out.stdout);
                Some(help.contains("--offline"))
            })()
            .unwrap_or(false);
            let mut cmd = std::process::Command::new(&cli_path);
            cmd.arg("--plan-file").arg(&plan_path);
            if offline_supported {
                cmd.arg("--offline");
            } else {
                let _ = log_tx.send("INFO: offline flag not supported by current flows-function-demo binary (rebuild to enable no-LLM reruns)".into());
            }
            cmd.envs(std::env::vars());
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
            let start_res = cmd.spawn();
            match start_res {
                Ok(mut child) => {
                    let stdout = child.stdout.take();
                    let stderr = child.stderr.take();
                    let tx_clone = log_tx.clone();
                    if let Some(out) = stdout {
                        let txo = log_tx.clone();
                        thread::spawn(move || {
                            use std::io::{BufRead, BufReader};
                            let reader = BufReader::new(out);
                            for line in reader.lines().map_while(|r| r.ok()) {
                                let _ = txo.send(line);
                            }
                        });
                    }
                    if let Some(err) = stderr {
                        thread::spawn(move || {
                            use std::io::{BufRead, BufReader};
                            let reader = BufReader::new(err);
                            for line in reader.lines().map_while(|r| r.ok()) {
                                let _ = tx_clone.send(line);
                            }
                        });
                    }
                    let status = child.wait();
                    let _ = log_tx.send(format!(
                        "DONE: {:?}",
                        status.ok().map(|s| if s.success() { "ok" } else { "fail" })
                    ));
                }
                Err(e) => {
                    let _ = log_tx.send(format!("ERROR: failed to spawn CLI: {e}"));
                    let _ = log_tx.send("DONE: \"err\"".into());
                }
            }
            let _ = log_tx.send("REFRESH_ARTIFACTS".into());
        });
    }

    fn resolve_artifact_path(&self, file_name: &str) -> Option<String> {
        let flat = format!("{}/{}", self.artifacts_dir, file_name);
        if std::path::Path::new(&flat).exists() {
            return Some(flat);
        }

        let mut candidates: Vec<(std::time::SystemTime, String)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.artifacts_dir) {
            for e in rd.filter_map(|e| e.ok()) {
                let p = e.path();
                if p.is_dir() {
                    if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                        if fname.starts_with("run_") {
                            let candidate = p.join(file_name);
                            if candidate.exists() {
                                let modified = e
                                    .metadata()
                                    .and_then(|m| m.modified())
                                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                                candidates
                                    .push((modified, candidate.to_string_lossy().to_string()));
                            }
                        }
                    }
                }
            }
        }
        if candidates.is_empty() {
            None
        } else {
            candidates.sort_by(|a, b| b.0.cmp(&a.0));
            Some(candidates[0].1.clone())
        }
    }

    fn list_artifact_files(dir: &str) -> Vec<String> {
        let mut files: Vec<String> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.filter_map(|e| e.ok()) {
                let p = e.path();
                if p.is_dir() {
                    if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                        if fname.starts_with("run_") {
                            if let Ok(sub) = std::fs::read_dir(&p) {
                                for f in sub.filter_map(|f| f.ok()) {
                                    if let Some(inner) = f.file_name().to_str() {
                                        files.push(inner.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if files.is_empty() {
            if let Ok(rd2) = std::fs::read_dir(dir) {
                for e in rd2.filter_map(|e| e.ok()) {
                    if let Some(n) = e.path().file_name().and_then(|n| n.to_str()) {
                        files.push(n.to_string());
                    }
                }
            }
        }
        files.sort();
        files
    }

    fn refresh_artifacts(&mut self) {
        let mut current = Self::list_artifact_files(&self.artifacts_dir);
        current.retain(|p| {
            let base = p.rsplit('/').next().unwrap_or(p);
            (self.filter_plan && base.starts_with("plan_"))
                || (self.filter_wat && base.starts_with("wat_"))
                || (!base.starts_with("plan_") && !base.starts_with("wat_"))
        });
        let prev_set: std::collections::HashSet<_> =
            self.last_known_files.iter().cloned().collect();
        let mut new = Vec::new();
        for f in &current {
            if !prev_set.contains(f) {
                new.push(f.clone());
            }
        }
        self.artifacts = current.clone();
        self.new_artifacts = new;
        self.last_known_files = current;
        if let Some(sel) = &self.selected_artifact {
            if !self.artifacts.contains(sel) {
                self.selected_artifact = None;
                self.artifact_preview.clear();
            }
        }
        if self.selected_artifact.is_none() {
            if let Some(last) = self.artifacts.last() {
                self.selected_artifact = Some(last.clone());
                self.load_artifact_preview();
            }
        }
    }

    fn load_artifact_preview(&mut self) {
        if let Some(file_owned) = self.selected_artifact.clone() {
            let mut path = format!("{}/{}", self.artifacts_dir, file_owned);
            if !std::path::Path::new(&path).exists() {
                if let Ok(rd) = std::fs::read_dir(&self.artifacts_dir) {
                    let mut candidates: Vec<(std::time::SystemTime, String)> = Vec::new();
                    for e in rd.filter_map(|e| e.ok()) {
                        let p = e.path();
                        if p.is_dir() {
                            if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                                if fname.starts_with("run_") {
                                    let candidate = p.join(&file_owned);
                                    if candidate.exists() {
                                        let modified = e
                                            .metadata()
                                            .and_then(|m| m.modified())
                                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                                        candidates.push((
                                            modified,
                                            candidate.to_string_lossy().to_string(),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    if !candidates.is_empty() {
                        candidates.sort_by(|a, b| b.0.cmp(&a.0));
                        path = candidates[0].1.clone();
                    }
                }
            }
            match std::fs::read_to_string(&path) {
                Ok(data) => {
                    let mut s = data;
                    if s.len() > 64 * 1024 {
                        s.truncate(64 * 1024);
                        s.push_str("\n...<truncated>");
                    }
                    self.artifact_preview = s;
                    if file_owned.starts_with("plan_") {
                        self.maybe_parse_graph(&file_owned, &path);
                    } else {
                        self.show_graph_window = false;
                        self.plan_nodes.clear();
                        self.plan_edges.clear();
                        self.ir_nodes.clear();
                        self.ir_edges.clear();
                    }
                }
                Err(_) => {
                    self.artifact_preview = "<unreadable>".into();
                    self.show_graph_window = false;
                }
            }
        } else {
            self.show_graph_window = false;
        }
    }

    fn maybe_parse_graph(&mut self, file: &str, path: &str) {
        if !file.starts_with("plan_") {
            return;
        }
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => return,
        };
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        let content_hash = hasher.finish();
        if self.graph_last_file.as_deref() == Some(file)
            && self.graph_last_hash == Some(content_hash)
        {
            return;
        }
        let parsed = match serde_json::from_str::<Value>(&text) {
            Ok(v) => v,
            Err(_) => return,
        };
        let mut plan_nodes: Vec<GraphNode> = Vec::new();
        let mut plan_edges: Vec<GraphEdge> = Vec::new();
        let mut id_index: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        if let Some(flow) = parsed.get("flow") {
            if let Some(blocks) = flow.get("blocks").and_then(|b| b.as_array()) {
                for (i, b) in blocks.iter().enumerate() {
                    let id = b.get("id").and_then(|v| v.as_str()).unwrap_or("block");
                    let btype = b.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                    let next = b.get("next").and_then(|v| v.as_str()).unwrap_or("-");
                    let label = format!("{btype}\nnext={next}");
                    plan_nodes.push(GraphNode {
                        id: format!("flow:{id}"),
                        kind: format!("flow:{btype}"),
                        label,
                    });
                    if i > 0 {
                        plan_edges.push(GraphEdge { from: i - 1, to: i });
                    }
                    id_index.insert(format!("flow:{id}"), i);
                }
            }
        }
        if let Some(graph_val) = parsed.get("execution_graph") {
            let nodes_opt = graph_val
                .get("nodes")
                .and_then(|v| v.as_array())
                .or_else(|| graph_val.as_array());
            if let Some(nodes) = nodes_opt {
                let base = plan_nodes.len();
                for (i, n) in nodes.iter().enumerate() {
                    let kind = n.get("type").and_then(|v| v.as_str()).unwrap_or("node");
                    let id = n.get("id").and_then(|v| v.as_str()).unwrap_or(kind);
                    let label = match kind {
                        "range_scan" => {
                            let end = n.get("end").and_then(|v| v.as_u64()).unwrap_or(0);
                            let shards = n.get("shards").and_then(|v| v.as_u64()).unwrap_or(0);
                            format!("end={end}\nshards={shards}")
                        }
                        "switch_scan" => {
                            let stages = n
                                .get("evaluators")
                                .and_then(|v| v.as_array())
                                .map(|a| a.len())
                                .unwrap_or(0);
                            format!("stages={stages}")
                        }
                        other => other.to_string(),
                    };
                    plan_nodes.push(GraphNode {
                        id: id.to_string(),
                        kind: kind.to_string(),
                        label,
                    });
                    if i > 0 {
                        plan_edges.push(GraphEdge {
                            from: base + i - 1,
                            to: base + i,
                        });
                    }
                    id_index.insert(id.to_string(), base + i);
                }
            }
        }
        if let Some(evals) = parsed.get("evaluators").and_then(|v| v.as_array()) {
            let base = plan_nodes.len();
            for (i, e) in evals.iter().enumerate() {
                let id = e.get("id").and_then(|v| v.as_str()).unwrap_or("eval");
                let etype = e.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                let src_len = e
                    .get("source")
                    .and_then(|v| v.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0);
                let label = format!("{etype}\nsource={src_len}ch");
                plan_nodes.push(GraphNode {
                    id: format!("eval:{id}"),
                    kind: format!("eval:{etype}"),
                    label,
                });
                if i > 0 {
                    plan_edges.push(GraphEdge {
                        from: base + i - 1,
                        to: base + i,
                    });
                }
                id_index.insert(format!("eval:{id}"), base + i);
            }
        }
        let mut ir_nodes: Vec<GraphNode> = Vec::new();
        let mut ir_edges: Vec<GraphEdge> = Vec::new();
        if let Some(funcs) = parsed.get("functions").and_then(|v| v.as_array()) {
            for f in funcs {
                let fname = f.get("name").and_then(|v| v.as_str()).unwrap_or("func");
                let entry_idx = ir_nodes.len();
                ir_nodes.push(GraphNode {
                    id: format!("fn:{fname}"),
                    kind: "ir:function".into(),
                    label: fname.into(),
                });
                let mut prev = Some(entry_idx);
                if let Some(ir) = f.get("ir") {
                    if let Some(params) = ir.get("params").and_then(|p| p.as_array()) {
                        for p in params {
                            let pname = p.get("name").and_then(|v| v.as_str()).unwrap_or("p");
                            let pty = p.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                            let idx = ir_nodes.len();
                            ir_nodes.push(GraphNode {
                                id: format!("param:{pname}"),
                                kind: "ir:param".into(),
                                label: format!("{pname}:{pty}"),
                            });
                            if let Some(pr) = prev {
                                ir_edges.push(GraphEdge { from: pr, to: idx });
                            }
                            prev = Some(idx);
                        }
                    }
                    if let Some(locals) = ir.get("locals").and_then(|p| p.as_array()) {
                        for l in locals {
                            let lname = l.get("name").and_then(|v| v.as_str()).unwrap_or("l");
                            let lty = l.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                            let idx = ir_nodes.len();
                            ir_nodes.push(GraphNode {
                                id: format!("local:{lname}"),
                                kind: "ir:local".into(),
                                label: format!("{lname}:{lty}"),
                            });
                            if let Some(pr) = prev {
                                ir_edges.push(GraphEdge { from: pr, to: idx });
                            }
                            prev = Some(idx);
                        }
                    }
                    if let Some(body) = ir
                        .get("body")
                        .and_then(|b| b.as_array())
                        .filter(|b| !b.is_empty())
                    {
                        let start_len = ir_nodes.len();
                        Self::collect_ir_ops(body, &mut ir_nodes, &mut ir_edges, &mut Vec::new());
                        if let Some(pr) = prev {
                            if start_len < ir_nodes.len() {
                                ir_edges.push(GraphEdge {
                                    from: pr,
                                    to: start_len,
                                });
                            }
                        }
                    }
                }
            }
        }
        if let Some(flow) = parsed.get("flow") {
            if let Some(blocks) = flow.get("blocks").and_then(|b| b.as_array()) {
                for b in blocks {
                    if b.get("type").and_then(|v| v.as_str()) == Some("compute") {
                        if let Some(id) = b.get("id").and_then(|v| v.as_str()) {
                            if let Some(expr) = b.get("expression").and_then(|v| v.as_str()) {
                                if let Some(&source_idx) = id_index.get(&format!("flow:{id}")) {
                                    if let Some(exec_ref) = expr.strip_prefix("execution_graph:") {
                                        if let Some(&target_idx) = id_index.get(exec_ref) {
                                            plan_edges.push(GraphEdge {
                                                from: source_idx,
                                                to: target_idx,
                                            });
                                        }
                                    } else if let Some(eval_ref) = expr.strip_prefix("eval:") {
                                        if let Some(&target_idx) =
                                            id_index.get(&format!("eval:{eval_ref}"))
                                        {
                                            plan_edges.push(GraphEdge {
                                                from: source_idx,
                                                to: target_idx,
                                            });
                                        }
                                    } else if expr.starts_with("function:") && !ir_nodes.is_empty()
                                    {
                                        plan_edges.push(GraphEdge {
                                            from: source_idx,
                                            to: 0,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if !plan_nodes.is_empty() || !ir_nodes.is_empty() {
            self.plan_nodes = plan_nodes;
            self.plan_edges = plan_edges;
            self.ir_nodes = ir_nodes;
            self.ir_edges = ir_edges;
            self.graph_last_file = Some(file.to_string());
            self.graph_last_hash = Some(content_hash);
            self.show_graph_window = true;
        }
    }

    fn collect_ir_ops(
        ops: &[Value],
        nodes: &mut Vec<GraphNode>,
        edges: &mut Vec<GraphEdge>,
        block_stack: &mut Vec<usize>,
    ) {
        let mut prev_idx: Option<usize> = None;
        for op in ops {
            let optype = op.get("op").and_then(|v| v.as_str()).unwrap_or("?");
            let idx = nodes.len();
            let mut label = optype.to_string();
            if let Some(val) = op.get("value") {
                if val.get("value").and_then(|v| v.as_i64()).is_some() {
                    label.push_str(" val");
                }
            }
            if let Some(target) = op.get("target").and_then(|v| v.as_str()) {
                label.push_str(&format!(" ->{target}"));
            }
            nodes.push(GraphNode {
                id: format!("op{idx}"),
                kind: format!("ir:{optype}"),
                label,
            });
            if let Some(p) = prev_idx {
                edges.push(GraphEdge { from: p, to: idx });
            }
            match optype {
                "BLOCK" => {
                    block_stack.push(idx);
                    if let Some(body) = op.get("body").and_then(|b| b.as_array()) {
                        Self::collect_ir_ops(body, nodes, edges, block_stack);
                    }
                    block_stack.pop();
                }
                "IF" => {
                    if let Some(thens) = op.get("then").and_then(|b| b.as_array()) {
                        let before_len = nodes.len();
                        Self::collect_ir_ops(thens, nodes, edges, block_stack);
                        if before_len < nodes.len() {
                            edges.push(GraphEdge {
                                from: idx,
                                to: before_len,
                            });
                        }
                    }
                }
                "BR" | "BR_IF" => {
                    if let Some(tdepth) = op.get("target").and_then(|v| v.as_u64()) {
                        if let Some(block_idx) = block_stack.iter().rev().nth(tdepth as usize) {
                            edges.push(GraphEdge {
                                from: idx,
                                to: *block_idx,
                            });
                        }
                    }
                }
                _ => {}
            }
            prev_idx = Some(idx);
        }
    }
}
