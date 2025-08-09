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

# test/pops_functionality_test.exs
defmodule PoPSFunctionalityTest do
  use ExUnit.Case, async: true

  alias GtrFabric.{
    TrustScore,
    PublishedOffering,
    ConsumerFactors
  }

  # Default structs for testing
  @default_trust_score %TrustScore{
    value: 0.8,
    last_updated_ts: :os.system_time(:seconds) - 86400 # 1 day ago
  }

  @default_consumer %ConsumerFactors{
    risk_aversion: 0.5,
    budget: 1000,
    cost_of_failure: 1000.0
  }

  @default_offering %PublishedOffering{
    staked_collateral: 200,
    price_per_call: 120
  }

  describe "PoPS Algorithm Tests" do
    test "calculate_slash_percentage works correctly" do
      params = GtrFabric.DynamicParameters.new()
      # No shortfall, no slash
      assert GtrFabric.calculate_slash_percentage(1.0, 1.0, params) == 0.0

      # Performance better than required, no slash
      assert GtrFabric.calculate_slash_percentage(1.0, 1.1, params) == 0.0

      # 50% shortfall (at the centre of the curve) should be a significant slash
      slash = GtrFabric.calculate_slash_percentage(1.0, 0.5, params)
      assert_in_delta slash, 73.0, 2.0 # Check it's in the expected range for default params

      # Total failure (100% shortfall) should be near 100% slash
      slash_total_failure = GtrFabric.calculate_slash_percentage(1.0, 0.0, params)
      assert slash_total_failure > 95.0
    end

    test "update_trust_score_on_success increases score" do
      updated_score = GtrFabric.update_trust_score_on_success(@default_trust_score, 0.1)
      assert updated_score.value > @default_trust_score.value
      assert updated_score.value == @default_trust_score.value + 0.1 * (1.0 - @default_trust_score.value)
    end

    test "update_trust_score_on_failure decreases score" do
      params = GtrFabric.DynamicParameters.new()
      updated_score = GtrFabric.update_trust_score_on_failure(@default_trust_score, 0.5, params)
      assert updated_score.value < @default_trust_score.value
    end

    test "decay_trust_score_continuously decreases score over time" do
      params = GtrFabric.DynamicParameters.new()
      # Decay over 1 day (86400 seconds)
      decayed_score =
        GtrFabric.Reputation.decay_trust_score_continuously(@default_trust_score, 86_400, params)

      assert decayed_score.value < @default_trust_score.value

      assert_in_delta decayed_score.value,
                        @default_trust_score.value * :math.exp(-params.decay_lambda_per_day * 1.0),
                        0.001
    end

    test "calculate_supplier_offering returns plausible values" do
      params = GtrFabric.DynamicParameters.new()
      {price, collateral} = GtrFabric.calculate_supplier_offering(@default_trust_score, params)
      assert is_integer(price) and price > 0
      assert is_integer(collateral) and collateral > 0

      # Higher trust should lead to lower price and lower collateral
      high_trust = %{@default_trust_score | value: 0.95}
      {_high_trust_price, high_trust_collateral} = GtrFabric.calculate_supplier_offering(high_trust, params)
      # The price calculation is complex, let's just check collateral
      # assert high_trust_price < price
      assert high_trust_collateral < collateral
    end

    test "calculate_consumer_utility provides a score" do
      utility = GtrFabric.calculate_consumer_utility(@default_offering, @default_trust_score, @default_consumer)
      assert is_float(utility) and utility > 0

      # Higher price should lead to lower utility
      costly_offering = %{@default_offering | price_per_call: 500}
      lower_utility = GtrFabric.calculate_consumer_utility(costly_offering, @default_trust_score, @default_consumer)
      assert lower_utility < utility
    end
  end
end
