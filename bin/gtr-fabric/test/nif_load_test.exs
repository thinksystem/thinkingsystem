# SPDX-License-Identifier: AGPL-3.0-only

if System.get_env("ENABLE_NIFS") == "1" do
  ExUnit.start()

  defmodule NifLoadTest do
    use ExUnit.Case, async: false

    # Helpers
    defp trust(v), do: %{__struct__: GtrFabric.TrustScore, value: v, last_updated_ts: 0}
    defp offering(coll, price), do: %{__struct__: GtrFabric.PublishedOffering, staked_collateral: coll, price_per_call: price}
    defp consumer(risk, budget, cof), do: %{__struct__: GtrFabric.ConsumerFactors, risk_aversion: risk, budget: budget, cost_of_failure: cof}
    defp params(), do: %{__struct__: GtrFabric.DynamicParameters, steepness: 5.0, centre: 0.3, failure_weight: 0.2, decay_lambda_per_day: 0.01, collateral_multiplier: 2.0, bonus_multiplier: 0.25}
    defp net_state(), do: %{__struct__: GtrFabric.NetworkState, network_failure_rate: 0.02, supply_demand_ratio: 1.1, avg_network_trust: 0.7}
    defp sla(), do: %{__struct__: GtrFabric.SLA, e2e_latency_ms: 120, jitter_ms: 10, loss_percentage: 0.5, weight_latency: 0.5, weight_throughput: 0.2, weight_trust: 0.3, multipath_threshold: 0.4}
    defp node_metrics(), do: %{__struct__: GtrFabric.NodeMetrics, trust_score: 0.8, available_throughput: 500.0, predicted_latency_to_target: 80.0}
    defp hop(id, pot, lat), do: %{__struct__: GtrFabric.CandidateHop, id: id, potential: pot, latency: lat}
    defp breadcrumb(id, ts), do: %{__struct__: GtrFabric.Breadcrumb, node_id: id, timestamp_ms: ts}

    test "calculate_consumer_utility_nif" do
      v = GtrFabric.CoreNifs.calculate_consumer_utility_nif(offering(1_000_000, 500_000), trust(0.85), consumer(0.3, 2_000_000, 5_000_000.0))
      assert is_number(v)
    end

    test "calculate_supplier_offering_nif" do
      {price, collateral} = GtrFabric.CoreNifs.calculate_supplier_offering_nif(trust(0.9), params())
      assert is_integer(price) and is_integer(collateral)
    end

    test "adjust_parameters_for_epoch_nif" do
      before_params = params()
      updated = GtrFabric.CoreNifs.adjust_parameters_for_epoch_nif(before_params, net_state())
      assert updated.steepness != before_params.steepness
    end

    test "calculate_slash_percentage_nif" do
      pct = GtrFabric.CoreNifs.calculate_slash_percentage_nif(0.95, 0.90, params())
      assert is_float(pct)
    end

    test "trust score updates success/failure/decay" do
      s0 = trust(0.8)
      s1 = GtrFabric.CoreNifs.update_trust_score_on_success_nif(s0, 0.05)
      s2 = GtrFabric.CoreNifs.update_trust_score_on_failure_nif(s1, 0.10, params())
      s3 = GtrFabric.CoreNifs.decay_trust_score_continuously_nif(s2, 3600, params())
      assert s3.value >= 0.0 and s3.value <= 1.0
    end

    test "calculate_potential_value" do
      {:ok, v} = GtrFabric.CoreNifs.calculate_potential_value(node_metrics(), sla())
      assert is_number(v)
    end

    test "calculate_forwarding_decision multipath" do
      {:ok, decision} = GtrFabric.CoreNifs.calculate_forwarding_decision([
        hop("a", 0.9, 50.0),
        hop("b", 0.85, 40.0)
      ], 0.01)
      assert is_binary(decision)
    end

    test "analyse_dag" do
      trails = [
        [breadcrumb("a", 1), breadcrumb("b", 2)],
        [breadcrumb("a", 3), breadcrumb("c", 4)]
      ]
      {:ok, report} = GtrFabric.CoreNifs.analyse_dag(trails, sla(), 10)
      assert is_boolean(report.sla_met) and is_number(report.avg_latency_ms)
    end

    test "create_trust_score_credential_nif (token may be invalid)" do
      perf = %{"success" => "10", "fail" => "1"}
      case GtrFabric.CoreNifs.create_trust_score_credential_nif("did:test:node", 0.77, perf, "dummy_token") do
        {:ok, vc} -> assert vc.issuer
        {:error, _} -> assert true
      end
    end

    test "calculate_consumer_utility_nif decodes maps with __struct__" do
      offering = %{__struct__: GtrFabric.PublishedOffering, staked_collateral: 1_000_000, price_per_call: 500_000}
      trust = %{__struct__: GtrFabric.TrustScore, value: 0.85, last_updated_ts: 0}
      consumer = %{__struct__: GtrFabric.ConsumerFactors, risk_aversion: 0.3, budget: 2_000_000, cost_of_failure: 5_000_000.0}
      val = GtrFabric.CoreNifs.calculate_consumer_utility_nif(offering, trust, consumer)
      assert is_number(val) and val >= 0
    end
  end
else
  IO.puts("NIF disabled: skipping nif_load_test.exs")
end
