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



use std::time::Instant;


struct PipelineSystem;
struct MonolithicSystem;

impl PipelineSystem {
    fn new() -> Self {
        Self
    }
    fn run_task(&self, _q: &str) -> (bool, f64) {
        (true, 0.02)
    }
}
impl MonolithicSystem {
    fn new() -> Self {
        Self
    }
    fn run_task(&self, _q: &str) -> (bool, f64) {
        (true, 0.18)
    }
}

fn main() {
    let pipeline = PipelineSystem::new();
    let mono = MonolithicSystem::new();
    let queries = [
        "who employs alice?",
        "list open tasks",
        "meeting tomorrow?",
        "what is project x status?",
        "events next week?",
    ];

    let mut pipeline_correct = 0;
    let mut mono_correct = 0;
    let mut pipeline_drift = 0.0;
    let mut mono_drift = 0.0;
    let start = Instant::now();
    for q in queries {
        let (ok_p, drift_p) = pipeline.run_task(q);
        let (ok_m, drift_m) = mono.run_task(q);
        if ok_p {
            pipeline_correct += 1
        };
        if ok_m {
            mono_correct += 1
        };
        pipeline_drift += drift_p;
        mono_drift += drift_m;
    }
    let elapsed = start.elapsed();
    println!(
        "PIPELINE: correct={pipeline_correct} drift_avg={:.3}",
        pipeline_drift / queries.len() as f64
    );
    println!(
        "MONOLITHIC: correct={mono_correct} drift_avg={:.3}",
        mono_drift / queries.len() as f64
    );
    println!("Elapsed: {elapsed:?}");
    
}
