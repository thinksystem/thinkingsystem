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

use crate::dynamic_parameters::DynamicParameters;
use crate::types::{
    Breadcrumb, CandidateHop, ConsumerFactors, NodeMetrics, PublishedOffering, ResolutionReport,
    Sla, TrustScore,
};
use rand::prelude::*;

pub fn calculate_potential_value_impl(metrics: NodeMetrics, sla: Sla) -> Result<f64, String> {
    if metrics.trust_score < 0.0 || metrics.trust_score > 1.0 {
        return Err(format!(
            "Invalid trust_score: {}. Must be between 0.0 and 1.0",
            metrics.trust_score
        ));
    }

    if metrics.available_throughput <= 0.0 {
        return Err(format!(
            "Invalid available_throughput: {}. Must be positive",
            metrics.available_throughput
        ));
    }

    if metrics.predicted_latency_to_target < 0.0 {
        return Err(format!(
            "Invalid predicted_latency_to_target: {}. Must be non-negative",
            metrics.predicted_latency_to_target
        ));
    }

    if !metrics.trust_score.is_finite()
        || !metrics.available_throughput.is_finite()
        || !metrics.predicted_latency_to_target.is_finite()
    {
        return Err("Invalid metrics: values must be finite (not NaN or infinite)".to_string());
    }

    let throughput_potential = sla.weight_throughput / metrics.available_throughput.max(1.0);
    let trust_potential = sla.weight_trust * (1.0 - metrics.trust_score);
    let latency_potential = sla.weight_latency * metrics.predicted_latency_to_target;

    Ok(throughput_potential + trust_potential + latency_potential)
}

pub fn calculate_forwarding_decision_impl(
    candidate_hops: Vec<CandidateHop>,
    multipath_threshold: f64,
) -> Result<String, String> {
    calculate_forwarding_decision_with_config(&candidate_hops, multipath_threshold)
}

pub fn calculate_forwarding_decision_with_config(
    candidate_hops: &[CandidateHop],
    multipath_threshold: f64,
) -> Result<String, String> {
    if candidate_hops.is_empty() {
        return Ok("loop".to_string());
    }

    for hop in candidate_hops {
        if !hop.potential.is_finite() {
            return Err(format!(
                "Invalid potential value for hop '{}': {}",
                hop.id, hop.potential
            ));
        }
        if !hop.latency.is_finite() {
            return Err(format!(
                "Invalid latency value for hop '{}': {}",
                hop.id, hop.latency
            ));
        }
        if hop.latency < 0.0 {
            return Err(format!(
                "Invalid latency for hop '{}': {}. Must be non-negative",
                hop.id, hop.latency
            ));
        }
    }

    let mut candidates_with_cost: Vec<(String, f64)> = candidate_hops
        .iter()
        .map(|hop| {
            let cost = hop.latency + hop.potential;
            (hop.id.clone(), cost)
        })
        .collect();

    candidates_with_cost.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let best_cost = match candidates_with_cost.first() {
        Some((_, cost)) => *cost,
        None => return Ok("loop".to_string()),
    };

    let viable_paths: Vec<(String, f64)> = candidates_with_cost
        .into_iter()
        .filter(|(_, cost)| *cost <= best_cost * multipath_threshold)
        .collect();

    if viable_paths.len() <= 1 {
        return Ok(viable_paths
            .first()
            .map_or("loop".to_string(), |(id, _)| id.clone()));
    }

    Ok(select_weighted_random_path(&viable_paths))
}

fn select_weighted_random_path(viable_paths: &[(String, f64)]) -> String {
    if viable_paths.is_empty() {
        return "loop".to_string();
    }

    if let Some((id, _)) = viable_paths.iter().find(|(_, cost)| *cost <= 0.0) {
        return id.clone();
    }

    let mut rng = thread_rng();
    let weights: Vec<f64> = viable_paths.iter().map(|(_, cost)| 1.0 / cost).collect();

    let total_weight: f64 = weights.iter().sum();

    if total_weight == 0.0 {
        return viable_paths[0].0.clone();
    }

    let mut cumulative_prob = 0.0;
    let random_draw = rng.gen_range(0.0..1.0);

    for (i, (id, _)) in viable_paths.iter().enumerate() {
        cumulative_prob += weights[i] / total_weight;
        if random_draw <= cumulative_prob {
            return id.clone();
        }
    }

    viable_paths[0].0.clone()
}

pub fn analyse_dag_impl(
    packet_trails: Vec<Vec<Breadcrumb>>,
    sla: Sla,
    total_packets_sent: u32,
) -> Result<ResolutionReport, String> {
    analyse_dag_with_trails(&packet_trails, &sla, total_packets_sent)
}

pub fn analyse_dag_with_trails(
    packet_trails: &[Vec<Breadcrumb>],
    sla: &Sla,
    total_packets_sent: u32,
) -> Result<ResolutionReport, String> {
    if total_packets_sent == 0 {
        return Err("Cannot analyse DAG: total_packets_sent cannot be zero.".to_string());
    }

    if packet_trails.is_empty() || packet_trails.iter().all(|trail| trail.is_empty()) {
        return Ok(ResolutionReport {
            sla_met: false,
            avg_latency_ms: -1.0,
            jitter_ms: -1.0,
            loss_percentage: 100.0,
            analysis_summary: "Analysis failed: No successful packets received.".to_string(),
        });
    }

    for (trail_index, trail) in packet_trails.iter().enumerate() {
        for (breadcrumb_index, breadcrumb) in trail.iter().enumerate() {
            if breadcrumb.node_id.is_empty() {
                return Err(format!(
                    "Invalid breadcrumb at trail {trail_index} position {breadcrumb_index}: node_id cannot be empty"
                ));
            }
        }
    }

    let mut latencies: Vec<f64> = Vec::new();
    for trail in packet_trails {
        if let Some(latency) = calculate_trail_latency(trail) {
            latencies.push(latency);
        }
    }

    if latencies.is_empty() {
        return Ok(ResolutionReport {
            sla_met: false,
            avg_latency_ms: -1.0,
            jitter_ms: -1.0,
            loss_percentage: 100.0,
            analysis_summary: "Analysis failed: No valid packet trails with measurable latency."
                .to_string(),
        });
    }

    let successful_packets = latencies.len() as u32;
    let loss_percentage = 100.0 * (1.0 - (successful_packets as f64 / total_packets_sent as f64));

    let total_latency: f64 = latencies.iter().sum();
    let avg_latency_ms = total_latency / successful_packets as f64;

    let variance: f64 = latencies
        .iter()
        .map(|l| {
            let diff = l - avg_latency_ms;
            diff * diff
        })
        .sum::<f64>()
        / successful_packets as f64;
    let jitter_ms = variance.sqrt();

    let latency_met = avg_latency_ms <= sla.e2e_latency_ms as f64;
    let jitter_met = jitter_ms <= sla.jitter_ms as f64;
    let loss_met = loss_percentage <= sla.loss_percentage as f64;
    let sla_met = latency_met && jitter_met && loss_met;

    let summary = format!(
        "SLA Check: Latency {:.2}ms (Req: {}ms) -> {}, Jitter {:.2}ms (Req: {}ms) -> {}, Loss {:.2}% (Req: {}%) -> {}. Overall: {}",
        avg_latency_ms, sla.e2e_latency_ms, latency_met,
        jitter_ms, sla.jitter_ms, jitter_met,
        loss_percentage, sla.loss_percentage, loss_met,
        if sla_met { "MET" } else { "FAILED" }
    );

    Ok(ResolutionReport {
        sla_met,
        avg_latency_ms,
        jitter_ms,
        loss_percentage,
        analysis_summary: summary,
    })
}

fn calculate_trail_latency(trail: &[Breadcrumb]) -> Option<f64> {
    if trail.len() < 2 {
        return None;
    }
    let start_time = trail.first().unwrap().timestamp_ms;
    let end_time = trail.last().unwrap().timestamp_ms;

    if end_time < start_time {
        return None;
    }

    const MAX_REASONABLE_LATENCY_MS: u64 = 3_600_000;
    let latency = end_time - start_time;
    if latency > MAX_REASONABLE_LATENCY_MS {
        return None;
    }

    Some(latency as f64)
}

pub fn calculate_slash_percentage(
    required_performance: f64,
    actual_performance: f64,
    params: &DynamicParameters,
) -> f64 {
    if actual_performance >= required_performance {
        return 0.0;
    }

    let shortfall = (required_performance - actual_performance) / required_performance;

    let penalty_factor = 1.0 / (1.0 + (-params.steepness * (shortfall - params.centre)).exp());

    penalty_factor * 100.0
}

pub fn update_trust_score_on_success(mut score: TrustScore, success_weight: f64) -> TrustScore {
    score.value += success_weight * (1.0 - score.value);
    score.last_updated_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    score
}

pub fn update_trust_score_on_failure(
    mut score: TrustScore,
    slash_percentage: f64,
    params: &DynamicParameters,
) -> TrustScore {
    let normalised_slash = slash_percentage / 100.0;
    score.value -= params.failure_weight * normalised_slash * score.value;
    score.last_updated_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    score
}

pub fn decay_trust_score_continuously(
    mut score: TrustScore,
    seconds_elapsed: u64,
    params: &DynamicParameters,
) -> TrustScore {
    let days_elapsed = seconds_elapsed as f64 / 86400.0;
    let decay_factor = (-params.decay_lambda_per_day * days_elapsed).exp();
    score.value *= decay_factor;
    score
}

pub fn calculate_supplier_offering(
    trust_score: &TrustScore,
    params: &DynamicParameters,
) -> (u64, u64) {
    let base_reward = 100;

    let reputation_bonus = if trust_score.value > 0.5 {
        (trust_score.value - 0.5) * 2.0 * params.bonus_multiplier
    } else {
        0.0
    };

    let final_reward = base_reward as f64 * (1.0 + reputation_bonus);

    let collateral_scaling_factor = (-params.steepness * trust_score.value).exp();
    let collateral =
        (base_reward as f64 * params.collateral_multiplier) * collateral_scaling_factor;

    (final_reward as u64, collateral as u64)
}

pub fn calculate_consumer_utility(
    offering: &PublishedOffering,
    trust_score: &TrustScore,
    consumer: &ConsumerFactors,
) -> f64 {
    let risk_factor = 1.0 - trust_score.value;
    let adjusted_cost = offering.price_per_call as f64 + (risk_factor * consumer.cost_of_failure);

    let collateral_value = (offering.staked_collateral as f64).ln_1p();
    let trust_value = trust_score.value.exp();
    let promised_performance = collateral_value * trust_value;

    if adjusted_cost <= 0.0 {
        return f64::INFINITY;
    }

    promised_performance / adjusted_cost
}
