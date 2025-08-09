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

#![allow(dead_code)]
#![allow(unused_variables)]

use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;

use estel::{
    chart_matcher, error::ChartError, ApiGraph, ChartSuggestionError, DataProfiler, DatasetSummary,
    DimensionProfile, ErrorReporter, MatchingConfig, ProfilingConfig, RenderSpec, Result,
};

use std::process::Command;

fn main() -> std::result::Result<(), eframe::Error> {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("Chart Suggestion Tool"),
        ..Default::default()
    };
    eframe::run_native(
        "Chart Suggestion Tool",
        options,
        Box::new(|_cc| Ok(Box::new(ChartSuggestionApp::new()))),
    )
}

#[derive(Debug, Clone, PartialEq)]
enum ActiveTab {
    Summary,
    Suggestions,
    Profiles,
}

struct ChartSuggestionApp {
    chart_html_files: std::collections::HashMap<usize, String>,
    selected_file: Option<PathBuf>,
    file_content: String,
    profiles: Vec<DimensionProfile>,
    dataset_summary: Option<DatasetSummary>,
    chart_suggestions: Vec<RenderSpec>,
    api_graph: Option<ApiGraph>,
    data_profiler: DataProfiler,
    profiling_config: ProfilingConfig,
    matching_config: MatchingConfig,
    active_tab: ActiveTab,
    show_config: bool,
    selected_chart: Option<usize>,
    error_message: Option<String>,
    error_reporter: ErrorReporter,
    runtime: Arc<Runtime>,
    is_processing: bool,
    progress_message: String,
    
    symbolic_scores: Vec<f64>,
    symbolic_notes: Vec<Vec<String>>,
    
    learned_scores: Vec<f64>,
    
    use_learned_ranking: bool,
    
    base_chart_suggestions: Vec<RenderSpec>,
    base_symbolic_scores: Vec<f64>,
    base_symbolic_notes: Vec<Vec<String>>,
    
    learned_chart_suggestions: Vec<RenderSpec>,
    learned_symbolic_scores: Vec<f64>,
    learned_symbolic_notes: Vec<Vec<String>>,
}

impl ChartSuggestionApp {
    fn new() -> Self {
        let runtime = Arc::new(Runtime::new().expect("Failed to create Tokio runtime"));
        let config_paths = [
            "config/plotly_api.yml",
            "bin/demos/estel-chart-demo/config/plotly_api.yml",
            "../../../crates/estel/config/plotly_api.yml",
            "crates/estel/config/plotly_api.yml",
        ];

        let api_graph = config_paths
            .iter()
            .find_map(|path| ApiGraph::from_yaml_file(path).ok());

        if api_graph.is_none() {
            eprintln!("Failed to load API graph from any of the following paths:");
            for path in &config_paths {
                eprintln!("- {path}");
            }
        }

        Self {
            selected_file: None,
            file_content: String::new(),
            profiles: Vec::new(),
            dataset_summary: None,
            chart_suggestions: Vec::new(),
            api_graph,
            data_profiler: DataProfiler::new(),
            profiling_config: ProfilingConfig::default(),
            matching_config: MatchingConfig::default(),
            active_tab: ActiveTab::Summary,
            show_config: false,
            selected_chart: None,
            error_message: None,
            error_reporter: ErrorReporter::new(),
            runtime,
            is_processing: false,
            progress_message: String::new(),
            chart_html_files: std::collections::HashMap::new(),
            symbolic_scores: Vec::new(),
            symbolic_notes: Vec::new(),
            learned_scores: Vec::new(),
            use_learned_ranking: true,
            base_chart_suggestions: Vec::new(),
            base_symbolic_scores: Vec::new(),
            base_symbolic_notes: Vec::new(),
            learned_chart_suggestions: Vec::new(),
            learned_symbolic_scores: Vec::new(),
            learned_symbolic_notes: Vec::new(),
        }
    }

    fn process_file(&mut self, file_path: PathBuf) {
        self.is_processing = true;
        self.progress_message = "Loading file...".to_string();
        self.error_message = None;
        self.profiles.clear();
        self.chart_suggestions.clear();
        self.dataset_summary = None;
        self.symbolic_scores.clear();
        self.symbolic_notes.clear();
        self.learned_scores.clear();
        self.base_chart_suggestions.clear();
        self.base_symbolic_scores.clear();
        self.base_symbolic_notes.clear();
        self.learned_chart_suggestions.clear();
        self.learned_symbolic_scores.clear();
        self.learned_symbolic_notes.clear();
        self.selected_chart = None;

        match self.analyse_file(file_path) {
            Ok(()) => {
                self.progress_message = "Analysis complete!".to_string();
            }
            Err(e) => {
                self.error_message = Some(self.error_reporter.report(&e));
                self.progress_message = "Analysis failed".to_string();
            }
        }
        self.is_processing = false;
    }

    fn apply_ranking_selection(&mut self) {
        
        self.selected_chart = None;
        if self.use_learned_ranking {
            self.chart_suggestions = self.learned_chart_suggestions.clone();
            self.symbolic_scores = self.learned_symbolic_scores.clone();
            self.symbolic_notes = self.learned_symbolic_notes.clone();
        } else {
            self.chart_suggestions = self.base_chart_suggestions.clone();
            self.symbolic_scores = self.base_symbolic_scores.clone();
            self.symbolic_notes = self.base_symbolic_notes.clone();
        }
    }

    fn analyse_file(&mut self, file_path: PathBuf) -> Result<()> {
        self.progress_message = "Analysing data structure...".to_string();
        self.profiles = self
            .data_profiler
            .profile_csv(&file_path)
            .map_err(|e| anyhow::anyhow!("Profiling failed: {}", e))?;

        self.progress_message = "Generating summary...".to_string();
        self.dataset_summary = Some(self.data_profiler.get_dataset_summary(&self.profiles));

        if let Some(api_graph) = &self.api_graph {
            self.progress_message = "Finding compatible charts...".to_string();
            self.chart_suggestions = chart_matcher::find_qualified_charts(
                &self.profiles,
                api_graph,
                &self.matching_config,
            );

            
            {
                use estel::symbolic_filtering::{
                    AnalysisGoal, ChartSpec, ChartType, ColumnProfile as SymColProfile,
                    DataType as SymDataType, GraphAwareSymbolicEngine,
                };

                self.symbolic_scores = Vec::with_capacity(self.chart_suggestions.len());
                self.symbolic_notes = Vec::with_capacity(self.chart_suggestions.len());

                for sugg in &self.chart_suggestions {
                    
                    let x_field = sugg.mappings.get("x").cloned().unwrap_or_default();
                    let y_fields = sugg
                        .mappings
                        .iter()
                        .filter_map(|(k, v)| if k.starts_with("y") { Some(v.clone()) } else { None })
                        .collect::<Vec<_>>();
                    let colour = sugg.mappings.get("color").cloned();

                    let mut col_profiles = std::collections::HashMap::new();
                    for p in &self.profiles {
                        let dt = match p.data_type {
                            estel::DataType::Numeric => SymDataType::Numeric,
                            estel::DataType::Categorical => SymDataType::Categorical,
                            estel::DataType::Temporal => SymDataType::Temporal,
                        };
                        col_profiles.insert(
                            p.name.clone(),
                            SymColProfile {
                                name: p.name.clone(),
                                data_type: dt,
                                cardinality: p.cardinality.map(|c| c as u64),
                                has_nulls: p.null_percentage > 0.0,
                            },
                        );
                    }

                    let chart_type = match sugg.chart_name.to_lowercase().as_str() {
                        s if s.contains("bar") => ChartType::Bar,
                        s if s.contains("line") => ChartType::Line,
                        s if s.contains("scatter") => ChartType::Scatter,
                        s if s.contains("pie") => ChartType::Pie,
                        s if s.contains("histogram") => ChartType::Histogram,
                        s if s.contains("box") => ChartType::BoxPlot,
                        _ => ChartType::Bar,
                    };

                    
                    let has_temporal = {
                        let mut any_temporal = false;
                        let mut check_field = |name: &str| {
                            if let Some(p) = self.profiles.iter().find(|dp| dp.name == name) {
                                if matches!(p.data_type, estel::DataType::Temporal) {
                                    any_temporal = true;
                                }
                            }
                        };
                        if !x_field.is_empty() { check_field(&x_field); }
                        for yf in &y_fields { check_field(yf); }
                        if let Some(ref c) = colour { check_field(c); }
                        any_temporal
                    };

                    let goal = match chart_type {
                        ChartType::Line => AnalysisGoal::ShowTrend,
                        ChartType::Histogram | ChartType::BoxPlot => AnalysisGoal::ShowDistribution,
                        ChartType::Scatter => AnalysisGoal::FindRelationship,
                        ChartType::Pie => AnalysisGoal::ShowComposition,
                        ChartType::Bar => if has_temporal { AnalysisGoal::ShowTrend } else { AnalysisGoal::Compare },
                    };

                    let spec = ChartSpec {
                        chart_type,
                        x_axis_field: x_field,
                        y_axis_fields: y_fields,
                        colour_field: colour,
                        column_profiles: col_profiles,
                    };
                    let engine = GraphAwareSymbolicEngine::default();
                    let (score, notes) = engine.enhanced_evaluate(&spec, &goal);
                    self.symbolic_scores.push(score);
                    self.symbolic_notes.push(notes);
                }
            }

            
            self.base_chart_suggestions = self.chart_suggestions.clone();
            self.base_symbolic_scores = self.symbolic_scores.clone();
            self.base_symbolic_notes = self.symbolic_notes.clone();

            
            {
                use estel::LearnedScorer;
                use std::collections::HashMap;

                fn spec_key(spec: &RenderSpec) -> String {
                    let mut parts: Vec<String> = spec
                        .mappings
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect();
                    parts.sort();
                    format!("{}|{}", spec.chart_name, parts.join(","))
                }

                let mut sym_score_by_key: HashMap<String, f64> = HashMap::new();
                let mut sym_notes_by_key: HashMap<String, Vec<String>> = HashMap::new();
                for (i, s) in self.base_chart_suggestions.iter().enumerate() {
                    let key = spec_key(s);
                    sym_score_by_key.insert(key.clone(), self.base_symbolic_scores[i]);
                    sym_notes_by_key.insert(key, self.base_symbolic_notes[i].clone());
                }

                let scorer = LearnedScorer::default_head();
                let reranked = scorer.rerank_specs(
                    &self.profiles,
                    self.base_chart_suggestions.clone(),
                    Some(&sym_score_by_key),
                );

                self.learned_chart_suggestions = reranked.iter().map(|(s, _)| s.clone()).collect();
                self.learned_scores = reranked.iter().map(|(_, sc)| *sc).collect();

                
                self.learned_symbolic_scores = Vec::with_capacity(self.learned_chart_suggestions.len());
                self.learned_symbolic_notes = Vec::with_capacity(self.learned_chart_suggestions.len());
                for s in &self.learned_chart_suggestions {
                    let key = spec_key(s);
                    self.learned_symbolic_scores
                        .push(*sym_score_by_key.get(&key).unwrap_or(&0.0));
                    self.learned_symbolic_notes
                        .push(sym_notes_by_key.get(&key).cloned().unwrap_or_default());
                }
            }

            
            self.apply_ranking_selection();
        }
        Ok(())
    }

    fn render_chart(&self, render_spec: &RenderSpec) -> Result<String> {
        use std::process::Command;
        let data_json = if let Some(ref path) = self.selected_file {
            let mut rdr = csv::Reader::from_path(path).map_err(std::io::Error::other)?;
            let mut data = std::collections::HashMap::new();
            let headers = rdr.headers().map_err(std::io::Error::other)?.clone();

            for header in headers.iter() {
                data.insert(header.to_string(), Vec::<String>::new());
            }

            for (i, result) in rdr.records().enumerate() {
                if i >= 100 {
                    break;
                }
                let record = result.map_err(std::io::Error::other)?;
                for (j, field) in record.iter().enumerate() {
                    if let Some(header) = headers.get(j) {
                        if let Some(column) = data.get_mut(header) {
                            column.push(field.to_string());
                        }
                    }
                }
            }
            serde_json::to_string(&data).unwrap_or_default()
        } else {
            r#"{"col1": [1, 2, 3, 4, 5], "col2": [10, 20, 30, 40, 50]}"#.to_string()
        };

        let mappings_json = serde_json::to_string(&render_spec.mappings).unwrap_or_default();

        let script = format!(
            r#"
import sys
import os
import json

candidate_paths = [
    'python_helpers',
    'bin/demos/estel-chart-demo/python_helpers',
    '../../../crates/estel/python_helpers',
    'crates/estel/python_helpers',
]
for p in candidate_paths:
    if os.path.isdir(p) and p not in sys.path:
        sys.path.insert(0, p)

from renderer import render_chart

data_json = r'''{data_json}'''

data = json.loads(data_json)

mappings_json = r'''{mappings_json}'''

mappings = json.loads(mappings_json)

try:
    result = render_chart('{chart}', json.dumps(data), mappings)
    print(result)
except Exception as e:
    import traceback
    traceback.print_exc(file=sys.stderr);
    sys.exit(1)
"#,
            chart = render_spec.chart_name
        );

        let output = Command::new("python3")
            .arg("-c")
            .arg(&script)
            .current_dir(".")
            .output()
            .map_err(ChartSuggestionError::Io)?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("Python script error: {stderr}");
            Err(ChartSuggestionError::Chart(ChartError::InvalidRenderSpec {
                name: render_spec.chart_name.clone(),
                reason: stderr.to_string(),
            }))
        }
    }
}

impl eframe::App for ChartSuggestionApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Chart Suggestion Tool");
                ui.separator();

                if ui.button(" Select CSV File").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("CSV files", &["csv"])
                        .pick_file()
                    {
                        self.selected_file = Some(path.clone());
                        self.process_file(path);
                    }
                }

                if let Some(ref path) = self.selected_file {
                    ui.label(format!("File: {}", path.display()));
                }

                ui.separator();

                if self.is_processing {
                    ui.spinner();
                    ui.label(&self.progress_message);
                }
            });
        });

        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Profiles: {}", self.profiles.len()));
                ui.label(format!("Suggestions: {}", self.chart_suggestions.len()));

                if let Some(ref summary) = self.dataset_summary {
                    ui.label(format!(
                        "Quality: {:.1}%",
                        summary.avg_quality_score * 100.0
                    ));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.toggle_value(&mut self.show_config, " Config");
                });
            });
        });

        if self.show_config {
            egui::SidePanel::left("config_panel").show(ctx, |ui| {
                ui.heading("Configuration");

                ui.collapsing("Ranking", |ui| {
                    let before = self.use_learned_ranking;
                    ui.checkbox(&mut self.use_learned_ranking, "Use learned ranking (reranked)");
                    if before != self.use_learned_ranking {
                        self.apply_ranking_selection();
                    }
                });

                ui.collapsing("Data Profiling", |ui| {
                    ui.label("Max categorical cardinality:");
                    ui.add(egui::Slider::new(
                        &mut self.profiling_config.max_categorical_cardinality,
                        10..=100,
                    ));

                    ui.label("Max sample values:");
                    ui.add(egui::Slider::new(
                        &mut self.profiling_config.max_sample_values,
                        5..=200,
                    ));

                    ui.checkbox(
                        &mut self.profiling_config.enable_advanced_stats,
                        "Enable advanced stats",
                    );

                    ui.label("Type confidence threshold:");
                    ui.add(egui::Slider::new(
                        &mut self.profiling_config.type_confidence_threshold,
                        0.0..=1.0,
                    ));
                });

                ui.collapsing("Chart Matching", |ui| {
                    ui.label("Quality threshold:");
                    ui.add(egui::Slider::new(
                        &mut self.matching_config.min_quality_score,
                        0.0..=1.0,
                    ));

                    ui.label("Max suggestions:");
                    ui.add(egui::Slider::new(
                        &mut self.matching_config.max_suggestions_per_chart,
                        1..=20,
                    ));

                    ui.checkbox(
                        &mut self.matching_config.include_partial_matches,
                        "Include partial matches",
                    );

                    ui.checkbox(
                        &mut self.matching_config.prefer_high_dimensionality,
                        "Prefer high dimensionality",
                    );
                });

                if ui.button("Reset to Defaults").clicked() {
                    self.profiling_config = ProfilingConfig::default();
                    self.matching_config = MatchingConfig::default();
                }
            });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(ref error) = self.error_message {
                ui.colored_label(egui::Color32::RED, "Error:");
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.monospace(error);
                });
                return;
            }

            if self.profiles.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.heading("Welcome to Chart Suggestion Tool");
                    ui.label("Select a CSV file to get started");
                });
                return;
            }

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, ActiveTab::Summary, " Summary");
                ui.selectable_value(&mut self.active_tab, ActiveTab::Suggestions, " Suggestions");
                ui.selectable_value(&mut self.active_tab, ActiveTab::Profiles, " Data Profiles");
            });
            ui.separator();

            match self.active_tab {
                ActiveTab::Summary => self.render_summary_tab(ui),
                ActiveTab::Suggestions => self.render_suggestions_tab(ui),
                ActiveTab::Profiles => self.render_profiles_tab(ui),
            }
        });
    }
}

impl ChartSuggestionApp {
    fn render_summary_tab(&self, ui: &mut egui::Ui) {
        if let Some(ref summary) = self.dataset_summary {
            ui.heading("Dataset Summary");

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.strong("Dimensions");
                    ui.label(format!("Total: {}", summary.total_dimensions));
                    ui.label(format!("Numeric: {}", summary.numeric_count));
                    ui.label(format!("Categorical: {}", summary.categorical_count));
                    ui.label(format!("Temporal: {}", summary.temporal_count));
                });

                ui.separator();

                ui.vertical(|ui| {
                    ui.strong("Quality");
                    ui.label(format!(
                        "Average Score: {:.1}%",
                        summary.avg_quality_score * 100.0
                    ));
                    ui.label(format!(
                        "Chart Readiness: {:.1}%",
                        summary.chart_readiness_score * 100.0
                    ));
                    ui.label(format!("Total Issues: {}", summary.total_issues));
                });

                ui.separator();

                ui.vertical(|ui| {
                    ui.strong("Potential Issues");
                    ui.label(format!("Total Issues: {}", summary.total_issues));
                    ui.label(format!(
                        "Chart Ready: {}",
                        if summary.chart_readiness_score > 0.6 {
                            "Yes"
                        } else {
                            "No"
                        }
                    ));
                });
            });

            ui.separator();

            let recommendations = summary.get_chart_recommendations();
            if !recommendations.is_empty() {
                ui.heading("Recommendations");
                for rec in &recommendations {
                    ui.label(format!("• {rec}"));
                }
            }
        }
    }

    fn render_suggestions_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Chart Suggestions");

        if self.chart_suggestions.is_empty() {
            ui.label("No chart suggestions available. Try adjusting configuration settings.");
            return;
        }

        let suggestions = self.chart_suggestions.clone();
        let mut actions = Vec::new();
        let mut rendered_charts: std::collections::HashMap<usize, Result<String>> =
            std::collections::HashMap::new();

        if let Some(selected_idx) = self.selected_chart {
            if let Some(suggestion) = suggestions.get(selected_idx) {
                rendered_charts.insert(selected_idx, self.render_chart(suggestion));
            }
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, suggestion) in suggestions.iter().enumerate() {
                let is_selected = self.selected_chart == Some(i);

                ui.push_id(i, |ui| {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.strong(&suggestion.chart_name);
                                    ui.label(format!("({}) ", suggestion.library));
                                    let quality_color = if suggestion.quality_score > 0.8 {
                                        egui::Color32::GREEN
                                    } else if suggestion.quality_score > 0.5 {
                                        egui::Color32::YELLOW
                                    } else {
                                        egui::Color32::RED
                                    };
                                    ui.colored_label(quality_color, format!("{:.1}%", suggestion.quality_score * 100.0));
                                    if i < self.learned_scores.len() {
                                        ui.separator();
                                        ui.label(format!("Learned: {:.1}%", self.learned_scores[i] * 100.0));
                                    }
                                });

                                ui.label(&suggestion.description);

                                egui::CollapsingHeader::new("Mappings")
                                    .id_salt(format!("mappings_{i}"))
                                    .show(ui, |ui| {
                                        for (arg, column) in &suggestion.mappings {
                                            ui.horizontal(|ui| {
                                                ui.label(format!("{arg}: "));
                                                ui.monospace(column);
                                            });
                                        }
                                    });

                                
                                if i < self.symbolic_scores.len() && i < self.symbolic_notes.len() {
                                    egui::CollapsingHeader::new("Symbolic Evaluation")
                                        .id_salt(format!("symb_{i}"))
                                        .show(ui, |ui| {
                                            let score = self.symbolic_scores[i];
                                            ui.label(format!("Score: {score:.2}"));
                                            if self.symbolic_notes[i].is_empty() {
                                                ui.label("No notes");
                                            } else {
                                                for note in &self.symbolic_notes[i] {
                                                    ui.label(format!("• {note}"));
                                                }
                                            }
                                        });
                                }
                            });

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(" Render").clicked() {
                                    actions.push(("render", i));
                                }
                                if ui.button(" Open in Browser").clicked() {
                                    actions.push(("open_browser", i));
                                }
                                if ui.button(" Generate HTML").clicked() {
                                    actions.push(("generate_html", i));
                                }
                                if ui.button(" Copy Config").clicked() {
                                    let json = format!(
                                        r#"{{"chart_name": "{}", "library": "{}", "mappings": {{{}}}}}"#,
                                        suggestion.chart_name, suggestion.library,
                                        suggestion.mappings.iter()
                                            .map(|(k, v)| format!("\"{k}\":\"{v}\""))
                                            .collect::<Vec<_>>().join(",")
                                    );
                                    ui.ctx().copy_text(json);
                                }
                            });
                        });

                        if is_selected {
                            ui.separator();
                            if let Some(render_result) = rendered_charts.get(&i) {
                                match render_result {
                                    Ok(rendered) => {
                                        ui.label(" Raw Chart JSON:");
                                        egui::ScrollArea::vertical()
                                            .max_height(300.0)
                                            .show(ui, |ui| {
                                                ui.monospace(rendered);
                                            });
                                    }
                                    Err(e) => {
                                        ui.colored_label(egui::Color32::RED, format!("Render error: {e}"));
                                    }
                                }
                            }

                            if self.chart_html_files.contains_key(&i) {
                                ui.separator();
                                if let Some(html_path) = self.chart_html_files.get(&i) {
                                    ui.horizontal(|ui| {
                                        ui.label(" HTML file:");
                                        ui.monospace(html_path);
                                        if ui.small_button(" Open").clicked() {
                                            let file_url = format!("file://{html_path}");
                                            let _ = Command::new("open")
                                                .arg(&file_url)
                                                .spawn();
                                        }
                                    });
                                    ui.label(" Click 'Open' to view the interactive chart!");
                                }
                            }
                        }
                    });
                });
                ui.separator();
            }
        });

        for (action, index) in actions {
            match action {
                "render" => {
                    self.selected_chart = Some(index);
                }
                "open_browser" => {
                    if let Some(suggestion) = suggestions.get(index) {
                        if let Err(e) = self.open_chart_in_browser(suggestion) {
                            self.error_message = Some(format!("Failed to open chart: {e}"));
                        }
                    }
                }
                "generate_html" => {
                    if let Some(suggestion) = suggestions.get(index) {
                        match self.generate_chart_html(suggestion, index) {
                            Ok(_) => {
                                self.selected_chart = Some(index);
                            }
                            Err(e) => {
                                self.error_message = Some(format!("Failed to generate HTML: {e}"));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn render_profiles_tab(&self, ui: &mut egui::Ui) {
        ui.heading("Data Profiles");

        if self.profiles.is_empty() {
            ui.label("No data profiles available.");
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, profile) in self.profiles.iter().enumerate() {
                ui.push_id(i, |ui| {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.strong(&profile.name);
                                    ui.label(format!("({:?})", profile.data_type));
                                    let quality_color = if profile.quality_score > 0.8 {
                                        egui::Color32::GREEN
                                    } else if profile.quality_score > 0.5 {
                                        egui::Color32::YELLOW
                                    } else {
                                        egui::Color32::RED
                                    };
                                    ui.colored_label(
                                        quality_color,
                                        format!("{:.1}%", profile.quality_score * 100.0),
                                    );
                                });

                                ui.horizontal(|ui| {
                                    ui.label(format!("Count: {}", profile.total_count));
                                    ui.label(format!(
                                        "Null: {:.1}%",
                                        profile.null_percentage * 100.0
                                    ));
                                    if let Some(cardinality) = profile.cardinality {
                                        ui.label(format!("Unique: {cardinality}"));
                                    }
                                });

                                if let Some(ref stats) = profile.numeric_stats {
                                    egui::CollapsingHeader::new("Numeric Stats")
                                        .id_salt(format!("numeric_{i}"))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                if let Some(min) = stats.min {
                                                    ui.label(format!("Min: {min:.2}"));
                                                }
                                                if let Some(max) = stats.max {
                                                    ui.label(format!("Max: {max:.2}"));
                                                }
                                                if let Some(mean) = stats.mean {
                                                    ui.label(format!("Mean: {mean:.2}"));
                                                }
                                            });
                                            ui.horizontal(|ui| {
                                                if let Some(std) = stats.std {
                                                    ui.label(format!("Std Dev: {std:.2}"));
                                                }
                                                if let Some(median) = stats.median {
                                                    ui.label(format!("Median: {median:.2}"));
                                                }
                                            });
                                        });
                                }

                                if let Some(ref stats) = profile.temporal_stats {
                                    egui::CollapsingHeader::new("Temporal Stats")
                                        .id_salt(format!("temporal_{i}"))
                                        .show(ui, |ui| {
                                            if let (Some(min_date), Some(max_date)) =
                                                (&stats.min_date, &stats.max_date)
                                            {
                                                ui.label(format!(
                                                    "Range: {min_date} to {max_date}"
                                                ));
                                            } else {
                                                ui.label("Date range: Not available");
                                            }
                                        });
                                }
                            });
                        });
                    });
                });
                ui.separator();
            }
        });
    }

    fn generate_chart_html(
        &mut self,
        render_spec: &RenderSpec,
        chart_index: usize,
    ) -> Result<String> {
        let data_json = self.get_chart_data_json()?;
        let mappings_json = serde_json::to_string(&render_spec.mappings)?;

        let script = format!(
            r#"
import sys
import os
import json

candidate_paths = [
    'python_helpers',
    'bin/demos/estel-chart-demo/python_helpers',
    '../../../crates/estel/python_helpers',
    'crates/estel/python_helpers',
]
for p in candidate_paths:
    if os.path.isdir(p) and p not in sys.path:
        sys.path.insert(0, p)

from renderer import create_temp_html_chart

data_json = r'''{data_json}'''

mappings_json = r'''{mappings_json}'''

try:
    data = json.loads(data_json)
    mappings = json.loads(mappings_json)
    html_path = create_temp_html_chart('{chart}', json.dumps(data), mappings)
    print(html_path)
except Exception as e:
    import traceback
    traceback.print_exc(file=sys.stderr);
    sys.exit(1)
"#,
            chart = render_spec.chart_name
        );

        let output = Command::new("python3")
            .arg("-c")
            .arg(&script)
            .current_dir(".")
            .output()
            .map_err(ChartSuggestionError::Io)?;

        if output.status.success() {
            let html_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            self.chart_html_files.insert(chart_index, html_path.clone());
            Ok(html_path)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ChartSuggestionError::Chart(ChartError::InvalidRenderSpec {
                name: render_spec.chart_name.clone(),
                reason: stderr.to_string(),
            }))
        }
    }

    fn open_chart_in_browser(&self, render_spec: &RenderSpec) -> Result<()> {
        let data_json = self.get_chart_data_json()?;
        let mappings_json = serde_json::to_string(&render_spec.mappings)?;

        let script = format!(
            r#"
import sys
import os
import json

candidate_paths = [
    'python_helpers',
    'bin/demos/estel-chart-demo/python_helpers',
    '../../../crates/estel/python_helpers',
    'crates/estel/python_helpers',
]
for p in candidate_paths:
    if os.path.isdir(p) and p not in sys.path:
        sys.path.insert(0, p)

from renderer import open_chart_in_browser

data_json = r'''{data_json}'''

mappings_json = r'''{mappings_json}'''

try:
    data = json.loads(data_json)
    mappings = json.loads(mappings_json)
    html_path = open_chart_in_browser('{chart}', json.dumps(data), mappings)
    print("Chart opened: " + str(html_path))
except Exception as e:
    import traceback
    traceback.print_exc(file=sys.stderr);
    sys.exit(1)
"#,
            chart = render_spec.chart_name
        );

        let output = Command::new("python3")
            .arg("-c")
            .arg(&script)
            .current_dir(".")
            .output()
            .map_err(ChartSuggestionError::Io)?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ChartSuggestionError::Chart(ChartError::InvalidRenderSpec {
                name: render_spec.chart_name.clone(),
                reason: stderr.to_string(),
            }))
        }
    }

    fn get_chart_data_json(&self) -> Result<String> {
        if let Some(ref path) = self.selected_file {
            let mut rdr = csv::Reader::from_path(path).map_err(std::io::Error::other)?;
            let mut data = std::collections::HashMap::new();
            let headers = rdr.headers().map_err(std::io::Error::other)?.clone();

            for header in headers.iter() {
                data.insert(header.to_string(), Vec::<String>::new());
            }

            for (i, result) in rdr.records().enumerate() {
                if i >= 100 {
                    break;
                }
                let record = result.map_err(std::io::Error::other)?;
                for (j, field) in record.iter().enumerate() {
                    if let Some(header) = headers.get(j) {
                        if let Some(column) = data.get_mut(header) {
                            column.push(field.to_string());
                        }
                    }
                }
            }
            let json = serde_json::to_string(&data).unwrap_or_default();
            Ok(json)
        } else {
            Ok(r#"{"col1": [1, 2, 3, 4, 5], "col2": [10, 20, 30, 40, 50]}"#.to_string())
        }
    }
}
