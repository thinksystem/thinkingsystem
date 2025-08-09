# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2024 Jonathan Lee
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License version 3
# as published by the Free Software Foundation.
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
# See the GNU Affero General Public License for more details.
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see https://www.gnu.org/licenses/.

defmodule GtrFabric.DynamicPoPSTest do
  use ExUnit.Case, async: true

  alias GtrFabric.{DynamicParameters, NetworkState, TrustScore}
  alias GtrFabric

  # Helper to create a default set of dynamic parameters
  defp default_params do
    %DynamicParameters{
      steepness: 5.0,
      centre: 0.3,
      failure_weight: 0.2,
      decay_lambda_per_day: 0.01,
      collateral_multiplier: 2.0,
      bonus_multiplier: 0.25
    }
  end

  describe "adjust_parameters_for_epoch/2" do
    test "when network failure rate is high, penalties become stricter" do
      params = default_params()
      # Simulate a high failure rate (10%) and balanced supply/demand
      state = %NetworkState{
        network_failure_rate: 0.10,
        supply_demand_ratio: 1.0,
        avg_network_trust: 0.7
      }

      new_params = GtrFabric.adjust_parameters_for_epoch(params, state)

      assert new_params.steepness > params.steepness
      assert new_params.failure_weight > params.failure_weight
    end

    test "when network is healthy, penalties relax" do
      params = default_params()
      # Simulate a very low failure rate (1%) and balanced supply/demand
      state = %NetworkState{
        network_failure_rate: 0.01,
        supply_demand_ratio: 1.0,
        avg_network_trust: 0.9
      }

      new_params = GtrFabric.adjust_parameters_for_epoch(params, state)

      assert new_params.steepness < params.steepness
      assert new_params.failure_weight < params.failure_weight
    end

    test "when there is a supplier shortage, incentives increase" do
      params = default_params()
      # Simulate a supplier shortage (demand > supply) and healthy failure rate
      state = %NetworkState{
        network_failure_rate: 0.03,
        supply_demand_ratio: 0.8,
        avg_network_trust: 0.8
      }

      new_params = GtrFabric.adjust_parameters_for_epoch(params, state)

      assert new_params.bonus_multiplier > params.bonus_multiplier
      assert new_params.collateral_multiplier < params.collateral_multiplier
    end

    test "when there is a supplier surplus, requirements increase" do
      params = default_params()
      # Simulate a supplier surplus and healthy failure rate
      state = %NetworkState{
        network_failure_rate: 0.03,
        supply_demand_ratio: 2.0,
        avg_network_trust: 0.8
      }

      new_params = GtrFabric.adjust_parameters_for_epoch(params, state)

      assert new_params.bonus_multiplier < params.bonus_multiplier
      assert new_params.collateral_multiplier > params.collateral_multiplier
      assert new_params.decay_lambda_per_day > params.decay_lambda_per_day
    end

    test "parameters are clamped within reasonable bounds" do
      # Start with extreme params and push them further
      params = %DynamicParameters{
        steepness: 15.0,
        centre: 0.3,
        failure_weight: 0.1,
        decay_lambda_per_day: 0.05,
        collateral_multiplier: 0.5,
        bonus_multiplier: 1.0
      }

      # State that will push params to their limits
      state = %NetworkState{
        network_failure_rate: 0.15, # Push steepness up
        supply_demand_ratio: 0.5,   # Push collateral down, bonus up
        avg_network_trust: 0.5
      }

      new_params = GtrFabric.adjust_parameters_for_epoch(params, state)

      # Assert they don't exceed the hard-coded clamps in Rust
      assert new_params.steepness == 15.0
      assert new_params.collateral_multiplier == 0.5
      assert new_params.bonus_multiplier == 1.0
      assert new_params.decay_lambda_per_day < params.decay_lambda_per_day # Should decrease
    end
  end

  describe "PoPS functions with DynamicParameters" do
    test "calculate_supplier_offering uses dynamic params" do
      trust_score = %TrustScore{value: 0.8, last_updated_ts: :os.system_time(:seconds)}
      params = default_params()

      {reward, collateral} = GtrFabric.calculate_supplier_offering(trust_score, params)

      # These values depend on the placeholder base_reward in Rust, but we can check they are reasonable
      assert is_integer(reward) and reward > 0
      assert is_integer(collateral) and collateral > 0

      # Create modified params to see if the offering changes
      modified_params = %{params | collateral_multiplier: params.collateral_multiplier * 2}
      {_reward2, collateral2} = GtrFabric.calculate_supplier_offering(trust_score, modified_params)

      assert collateral2 > collateral
    end

    test "calculate_slash_percentage uses dynamic params" do
      params = default_params()
      slash = GtrFabric.calculate_slash_percentage(1.0, 0.5, params)
      assert slash > 0

      # Make the penalty curve steeper and check that the slash increases
      modified_params = %{params | steepness: params.steepness * 2}
      steeper_slash = GtrFabric.calculate_slash_percentage(1.0, 0.5, modified_params)
      assert steeper_slash > slash
    end
  end
end
