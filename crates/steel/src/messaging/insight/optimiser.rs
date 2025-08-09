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

use crate::messaging::insight::analysis::ContentAnalyser;
use crate::messaging::insight::config::ScoringConfig;
use crate::messaging::insight::distribution::ScoreDistribution;
use crate::messaging::insight::metrics::{ModelPerformance, TrainingExample};

pub struct ModelOptimiser {
    training_data: Vec<TrainingExample>,
    best_config: ScoringConfig,
    best_performance: ModelPerformance,
    best_distribution: ScoreDistribution,
}

impl ModelOptimiser {
    pub fn new() -> Self {
        Self {
            training_data: Vec::new(),
            best_config: ScoringConfig::default(),
            best_performance: ModelPerformance::new(),
            best_distribution: ScoreDistribution::default(),
        }
    }

    pub fn add_training_example(&mut self, example: TrainingExample) {
        self.training_data.push(example);
    }

    pub fn evaluate_config(&self, config: &ScoringConfig) -> (ModelPerformance, ScoreDistribution) {
        let analyser = ContentAnalyser::new(*config);
        let mut performance = ModelPerformance::new();

        let mut distribution = ScoreDistribution::default();

        for example in &self.training_data {
            let analysis = analyser.analyse(&example.text, &mut distribution);
            performance.update_metrics(analysis.requires_scribes_review, example.is_sensitive);
        }

        (performance, distribution)
    }

    pub fn optimise_grid_search(&mut self) -> ScoringConfig {
        if self.training_data.is_empty() {
            return ScoringConfig::default();
        }

        (self.best_performance, self.best_distribution) = self.evaluate_config(&self.best_config);
        let mut best_f1 = self.best_performance.f1_score;
        let mut best_config = self.best_config;

        println!("Baseline F1 Score: {best_f1:.4}");

        let mut improvement_history = Vec::new();
        let mut configs_since_improvement = 0;
        let max_configs_without_improvement = 5000;
        let min_improvement_threshold = 0.001;

        let at_symbol_bonuses = [0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];
        let all_digits_bonuses = [0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
        let uuid_like_bonuses = [0.2, 0.3, 0.4, 0.5, 0.6, 0.7];
        let alphanumeric_bonuses = [0.1, 0.15, 0.2, 0.25, 0.3];
        let api_key_like_bonuses = [0.2, 0.3, 0.4, 0.5, 0.6];

        let total_configs = at_symbol_bonuses.len()
            * all_digits_bonuses.len()
            * uuid_like_bonuses.len()
            * alphanumeric_bonuses.len()
            * api_key_like_bonuses.len();

        println!("Testing up to {total_configs} configurations with early stopping...");
        println!(
            "Will stop after {max_configs_without_improvement} configs without improvement > {min_improvement_threshold:.3}"
        );

        let mut configs_tested = 0;

        'outer: for &at_bonus in &at_symbol_bonuses {
            for &digits_bonus in &all_digits_bonuses {
                for &uuid_bonus in &uuid_like_bonuses {
                    for &alnum_bonus in &alphanumeric_bonuses {
                        for &api_bonus in &api_key_like_bonuses {
                            let config = ScoringConfig {
                                at_symbol_bonus: at_bonus,
                                all_digits_bonus: digits_bonus,
                                uuid_like_bonus: uuid_bonus,
                                alphanumeric_bonus: alnum_bonus,
                                api_key_like_bonus: api_bonus,
                                ..Default::default()
                            };

                            let (performance, distribution) = self.evaluate_config(&config);
                            configs_tested += 1;
                            configs_since_improvement += 1;

                            if configs_tested % 500 == 0 {
                                println!("Tested {configs_tested}/{total_configs} configs, current best F1: {best_f1:.4}, configs since improvement: {configs_since_improvement}");
                            }

                            let improvement = performance.f1_score - best_f1;
                            if improvement > min_improvement_threshold {
                                best_f1 = performance.f1_score;
                                best_config = config;
                                self.best_performance = performance;
                                self.best_distribution = distribution;
                                configs_since_improvement = 0;

                                improvement_history.push((configs_tested, best_f1));
                                println!("New best F1: {best_f1:.4} (+{improvement:.4}) at config #{configs_tested} - at_symbol={at_bonus:.2}, digits={digits_bonus:.2}, uuid={uuid_bonus:.2}, alnum={alnum_bonus:.2}, api={api_bonus:.2}");
                            }

                            if configs_since_improvement >= max_configs_without_improvement {
                                println!(
                                    " Early stopping: No improvement for {max_configs_without_improvement} configurations"
                                );
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }

        println!("\n Optimisation Results:");
        println!("Configurations tested: {configs_tested}");
        println!("Improvements found: {}", improvement_history.len());

        if !improvement_history.is_empty() {
            println!("\n Improvement Curve:");
            for (config_num, f1_score) in &improvement_history {
                println!("Config #{config_num}: F1 = {f1_score:.4}");
            }

            let total_improvement =
                improvement_history.last().unwrap().1 - improvement_history.first().unwrap().1;
            println!("Total improvement: +{total_improvement:.4}");
        } else {
            println!("No improvements found over baseline");
        }

        self.best_config = best_config;
        best_config
    }

    pub fn generate_synthetic_training_data(&mut self, samples_per_type: usize) {
        self.training_data.clear();

        for i in 0..samples_per_type {
            let short_email = format!("a{}@b{}.co", i % 10, i % 10);
            self.add_training_example(TrainingExample::new(format!("Email: {short_email}"), true));

            let natural_email = format!(
                "Contact john.smith{}@company{}.internal for details",
                i,
                i % 5
            );
            self.add_training_example(TrainingExample::new(natural_email, true));

            let cc_parts = format!(
                "Card number is {} {} {} {}",
                4000 + i % 1000,
                1000 + i % 1000,
                2000 + i % 1000,
                3000 + i % 1000
            );
            self.add_training_example(TrainingExample::new(cc_parts, true));

            let non_standard_api = format!("auth_{:016x}", (i as u64) * 0x123);
            self.add_training_example(TrainingExample::new(
                format!("Token: {non_standard_api}"),
                true,
            ));

            let ssn_no_hyphens = format!("{:09}", 100000000 + (i * 123) % 900000000);
            self.add_training_example(TrainingExample::new(format!("SSN {ssn_no_hyphens}"), true));

            let phone_no_sep = format!("1{:010}", 2000000000u64 + (i as u64 * 12345) % 8000000000);
            self.add_training_example(TrainingExample::new(format!("Call {phone_no_sep}"), true));

            let bare_uuid = format!(
                "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
                i * 17,
                i % 65536,
                i % 65536,
                i % 65536,
                (i as u64) * 999
            );
            self.add_training_example(TrainingExample::new(format!("ID: {bare_uuid}"), true));

            let minimal_context = format!("{:016}", 5555000000000000u64 + i as u64);
            self.add_training_example(TrainingExample::new(minimal_context, true));

            let long_order = format!(
                "ORDER-{:016x}-{:08x}-BATCH-{:04x}",
                (i as u64) * 0xABC,
                i * 789,
                i % 10000
            );
            self.add_training_example(TrainingExample::new(
                format!("Order reference: {long_order}"),
                false,
            ));

            let system_id = format!(
                "SYS-{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
                i * 23,
                i % 65536,
                i % 65536,
                i % 65536,
                (i as u64) * 777
            );
            self.add_training_example(TrainingExample::new(
                format!("System identifier: {system_id}"),
                false,
            ));

            let build_email_like = format!("build-{:04}@jenkins-{:03}.internal.build", i, i % 1000);
            self.add_training_example(TrainingExample::new(
                format!("Build artefact: {build_email_like}"),
                false,
            ));

            let complex_version = format!(
                "{}.{}.{}.{:08}-{:016x}",
                i % 10,
                (i / 10) % 10,
                (i / 100) % 10,
                i * 12345,
                (i as u64) * 0x999
            );
            self.add_training_example(TrainingExample::new(
                format!("Software version: {complex_version}"),
                false,
            ));

            let db_conn = format!(
                "db-server-{:03}@cluster-{:02}.internal:{:04}",
                i % 1000,
                i % 100,
                5432 + i % 1000
            );
            self.add_training_example(TrainingExample::new(
                format!("Database endpoint: {db_conn}"),
                false,
            ));

            let log_file = format!("app-{:16x}-{:08}.log", (i as u64) * 0x111, i * 789);
            self.add_training_example(TrainingExample::new(format!("Log file: {log_file}"), false));

            let test_data = format!(
                "TEST_{:03}_{:02}_{:04}_DATA",
                100 + i % 900,
                10 + i % 90,
                1000 + i % 9000
            );
            self.add_training_example(TrainingExample::new(
                format!("Test dataset: {test_data}"),
                false,
            ));

            let config_key = format!("CONFIG_{:032X}", (i as u64) * 0x12345678);
            self.add_training_example(TrainingExample::new(
                format!("Configuration key: {config_key}"),
                false,
            ));

            let internal_email = format!("employee{i:03}@internal.testcorp.local");
            self.add_training_example(TrainingExample::new(
                format!("Internal contact: {internal_email}"),
                true,
            ));

            let service_token = format!("svc-{:08x}{:08x}", i * 111, i * 222);
            self.add_training_example(TrainingExample::new(
                format!("Service token: {service_token}"),
                true,
            ));

            let customer_ref = format!("CUST-{:06}-{:04}-{:02}", i * 17, i % 10000, i % 100);
            self.add_training_example(TrainingExample::new(
                format!("Customer ref: {customer_ref}"),
                false,
            ));

            let transaction_id = format!("TXN{:012x}{:04}", (i as u64) * 0x999, i % 10000);
            self.add_training_example(TrainingExample::new(
                format!("Transaction: {transaction_id}"),
                false,
            ));

            let ambiguous_number = format!("{:010}", 1000000000 + (i * 7919) % 9000000000);
            self.add_training_example(TrainingExample::new(
                format!("Reference number: {ambiguous_number}"),
                false,
            ));

            let url_with_at = format!(
                "https://api.service.com/users/user{}@domain{}/profile",
                i,
                i % 5
            );
            self.add_training_example(TrainingExample::new(
                format!("API URL: {url_with_at}"),
                false,
            ));

            let encoded_data = format!("data:{:032x}", (i as u64) * 0x987654321);
            self.add_training_example(TrainingExample::new(
                format!("Encoded value: {encoded_data}"),
                false,
            ));
        }

        let normal_phrases = [
            "Hello world",
            "Please review the document",
            "System status update",
            "Meeting scheduled for tomorrow",
            "File processed successfully",
            "User logged in",
            "Cache cleared",
            "Backup completed",
        ];

        for i in 0..samples_per_type / 4 {
            let phrase = normal_phrases[i % normal_phrases.len()];
            self.add_training_example(TrainingExample::new(phrase.to_string(), false));
        }

        for i in 0..(samples_per_type / 4) {
            let obvious_cc = format!("Credit card number: {}", 4000000000000000u64 + i as u64);
            self.add_training_example(TrainingExample::new(obvious_cc, true));

            let obvious_email = format!("Contact us at support@company{i}.com");
            self.add_training_example(TrainingExample::new(obvious_email, true));
        }
    }

    pub fn get_best_config(&self) -> &ScoringConfig {
        &self.best_config
    }
    pub fn get_best_performance(&self) -> &ModelPerformance {
        &self.best_performance
    }

    #[cfg(test)]
    pub fn training_data(&self) -> &Vec<TrainingExample> {
        &self.training_data
    }

    #[cfg(test)]
    pub fn best_distribution(&self) -> &ScoreDistribution {
        &self.best_distribution
    }
}

impl Default for ModelOptimiser {
    fn default() -> Self {
        Self::new()
    }
}
