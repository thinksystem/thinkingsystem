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

use rustler::NifStruct;

#[derive(Debug, Clone, NifStruct)]
#[module = "GtrFabric.CandidateHop"]
pub struct CandidateHop {
    pub id: String,
    pub potential: f64,
    pub latency: f64,
}

#[derive(Debug, Clone, NifStruct)]
#[module = "GtrFabric.NodeMetrics"]
pub struct NodeMetrics {
    pub trust_score: f64,
    pub available_throughput: f64,
    pub predicted_latency_to_target: f64,
}

#[derive(Clone, Debug, NifStruct)]
#[module = "GtrFabric.Breadcrumb"]
pub struct Breadcrumb {
    pub node_id: String,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, NifStruct)]
#[module = "GtrFabric.SLA"]
pub struct Sla {
    pub e2e_latency_ms: u32,
    pub jitter_ms: u32,
    pub loss_percentage: f32,

    pub weight_latency: f64,
    pub weight_throughput: f64,
    pub weight_trust: f64,

    pub multipath_threshold: f64,
}

#[derive(Debug, NifStruct)]
#[module = "GtrFabric.ResolutionReport"]
pub struct ResolutionReport {
    pub sla_met: bool,
    pub avg_latency_ms: f64,
    pub jitter_ms: f64,
    pub loss_percentage: f64,

    pub analysis_summary: String,
}

#[derive(Debug, Clone, NifStruct)]
#[module = "GtrFabric.TrustScore"]
pub struct TrustScore {
    pub value: f64,
    pub last_updated_ts: u64,
}

#[derive(Debug, Clone, NifStruct)]
#[module = "GtrFabric.PublishedOffering"]
pub struct PublishedOffering {
    pub staked_collateral: u64,
    pub price_per_call: u64,
}

#[derive(Debug, Clone, NifStruct)]
#[module = "GtrFabric.ConsumerFactors"]
pub struct ConsumerFactors {
    pub risk_aversion: f64,
    pub budget: u64,
    pub cost_of_failure: f64,
}
