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
#[module = "GtrFabric.NetworkState"]
pub struct NetworkState {

    pub network_failure_rate: f64,


    pub supply_demand_ratio: f64,

    pub avg_network_trust: f64,
}


#[derive(Debug, Clone, NifStruct)]
#[module = "GtrFabric.DynamicParameters"]
pub struct DynamicParameters {
    pub steepness: f64,
    pub centre: f64,
    pub failure_weight: f64,
    pub decay_lambda_per_day: f64,
    pub collateral_multiplier: f64,
    pub bonus_multiplier: f64,
}

impl Default for DynamicParameters {
    fn default() -> Self {

        Self {
            steepness: 5.0,
            centre: 0.3,
            failure_weight: 0.2,
            decay_lambda_per_day: 0.01,
            collateral_multiplier: 2.0,
            bonus_multiplier: 0.25,
        }
    }
}


pub fn adjust_parameters_for_epoch(
    current_params: &DynamicParameters,
    state: &NetworkState,
) -> DynamicParameters {
    let mut next_params = current_params.clone();


    if state.network_failure_rate > 0.05 {


        next_params.steepness *= 1.1;
        next_params.failure_weight *= 1.1;
    } else {

        next_params.steepness *= 0.95;
        next_params.failure_weight *= 0.95;
    }


    if state.supply_demand_ratio < 1.2 {


        next_params.bonus_multiplier *= 1.05;
        next_params.collateral_multiplier *= 0.98;
    } else {


        next_params.bonus_multiplier *= 0.98;
        next_params.collateral_multiplier *= 1.02;
    }


    if state.supply_demand_ratio > 1.5 {

        next_params.decay_lambda_per_day *= 1.1;
    } else {
        next_params.decay_lambda_per_day *= 0.95;
    }


    next_params.steepness = next_params.steepness.clamp(3.0, 15.0);
    next_params.failure_weight = next_params.failure_weight.clamp(0.1, 0.75);
    next_params.collateral_multiplier = next_params.collateral_multiplier.clamp(0.5, 5.0);
    next_params.bonus_multiplier = next_params.bonus_multiplier.clamp(0.1, 1.0);
    next_params.decay_lambda_per_day = next_params.decay_lambda_per_day.clamp(0.005, 0.05);

    next_params
}
