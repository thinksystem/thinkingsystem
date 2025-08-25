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


use serde::Serialize;

#[derive(Default, Serialize)]
pub struct WatStructuralMetrics {
    pub loops: u32,
    pub arithmetic_ops: u32,
    pub param_reads: u32,
    pub length_bytes: usize,
}


pub fn sanitize_wat_basic(input: &str) -> (String, WatStructuralMetrics) {
    let metrics = analyze_metrics(input);
    (input.to_string(), metrics)
}

fn analyze_metrics(wat: &str) -> WatStructuralMetrics {
    let mut loops = 0u32;
    let mut arithmetic_ops = 0u32;
    let mut param_reads = 0u32;
    for line in wat.lines() {
        let l = line.trim();
        if l.starts_with("(loop") {
            loops += 1;
        }
        if l.contains("f64.add")
            || l.contains("f64.sub")
            || l.contains("f64.mul")
            || l.contains("f64.div")
        {
            arithmetic_ops += 1;
        }
        if l.contains("local.get $") {
            param_reads += 1;
        }
    }
    WatStructuralMetrics {
        loops,
        arithmetic_ops,
        param_reads,
        length_bytes: wat.len(),
    }
}
