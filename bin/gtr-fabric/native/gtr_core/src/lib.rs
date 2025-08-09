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

pub mod core;
pub mod dynamic_parameters;
pub mod types;
pub mod vc_bridge;
pub mod vc_types;

// Re-export commonly used items
pub use dynamic_parameters::{adjust_parameters_for_epoch, DynamicParameters, NetworkState};
pub use types::{ConsumerFactors, PublishedOffering, TrustScore};

mod atoms {
    rustler::atoms! { ok, error, invalid_input }
}

#[rustler::nif]
fn calculate_potential_value(
    node_metrics: types::NodeMetrics,
    sla: types::Sla,
) -> Result<f64, String> {
    core::calculate_potential_value_impl(node_metrics, sla)
}

#[rustler::nif]
fn calculate_forwarding_decision(
    candidate_hops: Vec<types::CandidateHop>,
    multipath_threshold: f64,
) -> Result<String, String> {
    core::calculate_forwarding_decision_impl(candidate_hops, multipath_threshold)
}

#[rustler::nif]
fn analyse_dag(
    packet_trails: Vec<Vec<types::Breadcrumb>>,
    sla: types::Sla,
    total_packets_sent: u32,
) -> Result<types::ResolutionReport, String> {
    core::analyse_dag_impl(packet_trails, sla, total_packets_sent)
}

#[rustler::nif]
pub fn adjust_parameters_for_epoch_nif(
    current_params: DynamicParameters,
    state: NetworkState,
) -> DynamicParameters {
    adjust_parameters_for_epoch(&current_params, &state)
}

#[rustler::nif]
pub fn calculate_slash_percentage_nif(
    required_performance: f64,
    actual_performance: f64,
    params: DynamicParameters,
) -> f64 {
    core::calculate_slash_percentage(required_performance, actual_performance, &params)
}

#[rustler::nif]
pub fn update_trust_score_on_success_nif(score: TrustScore, success_weight: f64) -> TrustScore {
    core::update_trust_score_on_success(score, success_weight)
}

#[rustler::nif]
pub fn update_trust_score_on_failure_nif(
    score: TrustScore,
    slash_percentage: f64,
    params: DynamicParameters,
) -> TrustScore {
    core::update_trust_score_on_failure(score, slash_percentage, &params)
}

#[rustler::nif]
pub fn decay_trust_score_continuously_nif(
    score: TrustScore,
    seconds_elapsed: u64,
    params: DynamicParameters,
) -> TrustScore {
    core::decay_trust_score_continuously(score, seconds_elapsed, &params)
}

#[rustler::nif]
pub fn calculate_supplier_offering_nif(
    trust_score: TrustScore,
    params: DynamicParameters,
) -> (u64, u64) {
    core::calculate_supplier_offering(&trust_score, &params)
}

#[rustler::nif]
pub fn calculate_consumer_utility_nif(
    offering: PublishedOffering,
    trust_score: TrustScore,
    consumer: ConsumerFactors,
) -> f64 {
    core::calculate_consumer_utility(&offering, &trust_score, &consumer)
}

#[rustler::nif]
pub fn test_add(a: i64, b: i64) -> i64 {
    a + b
}

// Ensure something obvious we can grep for
#[no_mangle]
pub extern "C" fn gtr_core_marker() {}

rustler::init!("Elixir.GtrFabric.CoreNifs");
