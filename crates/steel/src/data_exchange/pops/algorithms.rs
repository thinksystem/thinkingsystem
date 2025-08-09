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



use super::shared_types::{
    ConsumerFactors, PenaltyCurve, PublishedOffering, SupplierPricingFactors, TrustScore,
};
use std::time::{SystemTime, UNIX_EPOCH};


pub fn calculate_slash_percentage(
    required_performance: f64,
    actual_performance: f64,
    curve: &PenaltyCurve,
) -> f64 {
    if required_performance <= 0.0 {
        return if actual_performance < 0.0 { 100.0 } else { 0.0 };
    }

    let deviation = (required_performance - actual_performance) / required_performance;
    let x = deviation.clamp(0.0, 1.0);

    100.0 / (1.0 + (-curve.steepness * (x - curve.centre)).exp())
}


pub fn update_trust_score_on_success(mut score: TrustScore, success_weight: f64) -> TrustScore {
    score.value += (1.0 - score.value) * success_weight;
    score.last_updated_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    score
}


pub fn update_trust_score_on_failure(
    mut score: TrustScore,
    slash_percentage: f64,
    failure_weight: f64,
) -> TrustScore {
    let slash_ratio = slash_percentage / 100.0;
    score.value *= 1.0 - (slash_ratio * failure_weight);
    score.last_updated_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    score
}


pub fn decay_trust_score(mut score: TrustScore, decay_lambda_per_day: f64) -> TrustScore {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let delta_t_secs = now.saturating_sub(score.last_updated_ts);
    let delta_t_days = delta_t_secs as f64 / 86400.0;

    score.value *= (-decay_lambda_per_day * delta_t_days).exp();

    score
}


pub fn calculate_supplier_offering(
    trust_score: &TrustScore,
    pricing: &SupplierPricingFactors,
    collateral_multiplier: f64,
) -> (u64, u64) {


    let collateral = (pricing.base_reward as f64
        * collateral_multiplier
        * (1.0 / (trust_score.value + 0.1))) as u64;


    let risk_premium = (1.0 - pricing.confidence) * collateral as f64;


    let reputation_bonus =
        trust_score.value * pricing.base_reward as f64 * pricing.reputation_bonus_multiplier;


    let final_reward = pricing.base_reward as f64 + risk_premium + reputation_bonus;

    (final_reward as u64, collateral)
}


pub fn calculate_consumer_utility(
    offering: &PublishedOffering,
    trust_score: &TrustScore,
    consumer: &ConsumerFactors,
) -> f64 {

    let promised_performance = offering.promised_guarantees.min_throughput;
    if promised_performance <= 0.0 {
        return 0.0;
    }


    let probability_of_failure = 1.0 - trust_score.value;
    let expected_loss = probability_of_failure * consumer.cost_of_failure;


    let adjusted_cost = offering.reward as f64 + expected_loss;
    if adjusted_cost <= 0.0 {
        return f64::MAX;
    }

    promised_performance / adjusted_cost
}
