# SPDX-License-Identifier: AGPL-3.0-only

ExUnit.start()

defmodule CoreWrapperTest do
  use ExUnit.Case, async: false

  @enable System.get_env("ENABLE_NIFS") == "1"

  test "consumer_utility unified shape" do
    offering = %{__struct__: GtrFabric.PublishedOffering, staked_collateral: 1_000_000, price_per_call: 500_000}
    trust = %{__struct__: GtrFabric.TrustScore, value: 0.85, last_updated_ts: 0}
    consumer = %{__struct__: GtrFabric.ConsumerFactors, risk_aversion: 0.3, budget: 2_000_000, cost_of_failure: 5_000_000.0}
    {tag, val} = GtrFabric.CoreWrapper.consumer_utility(offering, trust, consumer)
    assert tag in [:ok, :error]
    if @enable, do: assert(tag == :ok), else: assert(val == :nif_disabled)
  end
end
