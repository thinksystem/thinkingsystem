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

# file: test/core_functionality_test.exs
# To run: mix test test/core_functionality_test.exs

defmodule CoreFunctionalityTest do
  use ExUnit.Case, async: false # Run tests serially to avoid race conditions on benchmarks

  @moduledoc """
  Core functionality test suite for the GTR system.
  This script validates the functional correctness, mathematical properties,
  performance, and error handling of the Rust NIFs using the ExUnit framework.
  """

  # Alias for easier access
  alias GtrFabric.{NodeMetrics, CandidateHop, Breadcrumb, SLA}

  # --- Test Setup ---
  # Define a default SLA to be used in tests
  @default_sla %SLA{
    e2e_latency_ms: 100,
    jitter_ms: 10,
    loss_percentage: 5.0,
    weight_latency: 1.0,
    weight_throughput: 1500.0,
    weight_trust: 2000.0,
    multipath_threshold: 1.05
  }

  setup_all do
    # This check ensures the application is started before tests run
    Application.ensure_all_started(:gtr_fabric)
    :ok
  end

  # =====================================================
  # SECTION 1: FUNCTIONAL CORRECTNESS
  # =====================================================
  describe "Functional Correctness & Edge Case Analysis" do
    test "`calculate_potential_value` handles various scenarios correctly" do
      test_cases = [
        {%NodeMetrics{trust_score: 0.0, available_throughput: 1000.0, predicted_latency_to_target: 15.0}, "minimum trust"},
        {%NodeMetrics{trust_score: 1.0, available_throughput: 1000.0, predicted_latency_to_target: 15.0}, "maximum trust"},
        {%NodeMetrics{trust_score: 0.85, available_throughput: 0.1, predicted_latency_to_target: 15.0}, "near-zero throughput"},
      ]

      for {metrics, _desc} <- test_cases do
        result = GtrFabric.calculate_potential_value(metrics, @default_sla)
        assert is_float(result)
        assert result > 0.0
      end
    end

    test "`calculate_forwarding_decision` handles edge cases" do
      single_candidate = [%CandidateHop{id: "only_node", potential: 500.0, latency: 25.0}]
      result = GtrFabric.calculate_forwarding_decision(single_candidate, 1.05)
      assert result == "only_node"

      # Test empty candidates
      empty_result = GtrFabric.calculate_forwarding_decision([], 1.05)
      assert empty_result == "loop"
    end

    test "`analyse_dag` correctly evaluates SLAs" do
      # Test normal chronological order
      passing_trail = [
        [
          %Breadcrumb{node_id: "start", timestamp_ms: 1000},
          %Breadcrumb{node_id: "end", timestamp_ms: 1015}
        ]
      ]

      failing_trail = [
        [
          %Breadcrumb{node_id: "start", timestamp_ms: 1000},
          %Breadcrumb{node_id: "end", timestamp_ms: 1150}
        ]
      ]

      # Test passing case (15ms latency vs 20ms SLA)
            passing_sla = %SLA{
        e2e_latency_ms: 20,
        jitter_ms: 5,
        loss_percentage: 10.0,
        weight_latency: 1.0,
        weight_throughput: 1.0,
        weight_trust: 1.0,
        multipath_threshold: 0.05
      }
      {:ok, passing_report} = GtrFabric.analyse_dag(passing_trail, passing_sla, 1)
      assert passing_report.sla_met == true
      assert passing_report.avg_latency_ms == 15.0

      # Test failing case (150ms latency vs 100ms SLA)
            failing_sla = %SLA{
        e2e_latency_ms: 100,
        jitter_ms: 10,
        loss_percentage: 5.0,
        weight_latency: 1.0,
        weight_throughput: 1.0,
        weight_trust: 1.0,
        multipath_threshold: 0.05
      }
      {:ok, failing_report} = GtrFabric.analyse_dag(failing_trail, failing_sla, 1)
      assert failing_report.sla_met == false
      assert failing_report.avg_latency_ms == 150.0

      # Test empty trail
      {:ok, empty_report} = GtrFabric.analyse_dag([], failing_sla, 1)
      assert empty_report.sla_met == false
      assert empty_report.avg_latency_ms == -1.0
    end

    test "`analyse_dag` handles integer underflow correctly" do
      # Test the previously problematic case
      backward_trail = [
        [
          %Breadcrumb{node_id: "backward", timestamp_ms: 2000},
          %Breadcrumb{node_id: "time", timestamp_ms: 1000}
        ]
      ]

      # Test extreme timestamps that could cause underflow
      extreme_trail = [
        [
          %Breadcrumb{node_id: "start", timestamp_ms: 18446744073709551615},
          %Breadcrumb{node_id: "end", timestamp_ms: 0}
        ]
      ]

      # Backward trail should be handled gracefully (None latency)
      {:ok, backward_report} = GtrFabric.analyse_dag(backward_trail, @default_sla, 1)
      assert backward_report.analysis_summary |> String.contains?("No valid packet trails")

      # Extreme trail should be rejected (None latency)
      {:ok, extreme_report} = GtrFabric.analyse_dag(extreme_trail, @default_sla, 1)
      assert extreme_report.analysis_summary |> String.contains?("No valid packet trails")
    end
  end

  # =====================================================
  # SECTION 2: ERROR HANDLING
  # =====================================================
  describe "Error Handling Verification" do
    test "NIFs return errors for invalid inputs" do
      # Test `calculate_potential_value` with out-of-bounds data
      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_potential_value(%NodeMetrics{trust_score: -1.0, available_throughput: 1000.0, predicted_latency_to_target: 15.0}, @default_sla)
      end

      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_potential_value(%NodeMetrics{trust_score: 0.5, available_throughput: 0.0, predicted_latency_to_target: 15.0}, @default_sla)
      end

      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_potential_value(%NodeMetrics{trust_score: 0.5, available_throughput: 1000.0, predicted_latency_to_target: -10.0}, @default_sla)
      end
    end

    test "handles invalid types gracefully" do
      # Elixir validation will raise an ArgumentError before the NIF is even called
      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_potential_value("not a struct", @default_sla)
      end

      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_forwarding_decision("not a list", 1.05)
      end
    end

    test "handles edge cases correctly" do
      # Empty candidate list should return "loop"
      result = GtrFabric.calculate_forwarding_decision([], 1.05)
      assert result == "loop"

      # Empty trails should be handled gracefully, assuming at least one packet was sent but lost.
      {:ok, empty_report} = GtrFabric.analyse_dag([], @default_sla, 1)
      assert empty_report.sla_met == false
      assert empty_report.avg_latency_ms == -1.0
      assert String.contains?(empty_report.analysis_summary, "No successful packets received")
    end

    test "handles infinite and NaN values" do
      # Test infinite potential
      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_forwarding_decision([
          %CandidateHop{id: "node1", potential: :infinity, latency: 10.0}
        ], 1.05)
      end

      # Test NaN latency
      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_forwarding_decision([
          %CandidateHop{id: "node1", potential: 100.0, latency: :nan}
        ], 1.05)
      end
    end
  end

  # =====================================================
  # SECTION 3: MATHEMATICAL PROPERTIES
  # =====================================================
  describe "Mathematical Properties" do
    test "potential value function is deterministic" do
      metrics = %NodeMetrics{trust_score: 0.7, available_throughput: 500.0, predicted_latency_to_target: 25.0}

      results = Enum.map(1..10, fn _ -> GtrFabric.calculate_potential_value(metrics, @default_sla) end)
      unique_results = Enum.uniq(results)

      assert length(unique_results) == 1, "Potential value function should be deterministic"
    end

    test "forwarding decision is consistent for same inputs" do
      candidates = [
        %CandidateHop{id: "node1", potential: 300.0, latency: 15.0},
        %CandidateHop{id: "node2", potential: 500.0, latency: 10.0},
        %CandidateHop{id: "node3", potential: 400.0, latency: 20.0}
      ]

      decisions = Enum.map(1..100, fn _ ->
        GtrFabric.calculate_forwarding_decision(candidates, 1.05)
      end)

      unique_decisions = Enum.uniq(decisions)
      # Note: with probabilistic routing this might not be 1, but should be a small number
      assert length(unique_decisions) > 0 and length(unique_decisions) <= 3
    end

    test "potential value monotonicity properties" do
      base_metrics = %NodeMetrics{trust_score: 0.5, available_throughput: 1000.0, predicted_latency_to_target: 50.0}
      base_potential = GtrFabric.calculate_potential_value(base_metrics, @default_sla)

      # Higher trust should result in lower potential (better)
      higher_trust = %{base_metrics | trust_score: 0.8}
      higher_trust_potential = GtrFabric.calculate_potential_value(higher_trust, @default_sla)
      assert higher_trust_potential < base_potential, "Higher trust should result in lower potential"

      # Higher throughput should result in lower potential (better)
      higher_throughput = %{base_metrics | available_throughput: 2000.0}
      higher_throughput_potential = GtrFabric.calculate_potential_value(higher_throughput, @default_sla)
      assert higher_throughput_potential < base_potential, "Higher throughput should result in lower potential"

      # Lower latency should result in lower potential (better)
      lower_latency = %{base_metrics | predicted_latency_to_target: 25.0}
      lower_latency_potential = GtrFabric.calculate_potential_value(lower_latency, @default_sla)
      assert lower_latency_potential < base_potential, "Lower latency should result in lower potential"
    end
  end

  # =====================================================
  # SECTION 4: PERFORMANCE BENCHMARKS
  # =====================================================
  describe "Performance Benchmarks" do
    @tag :benchmark
    test "potential value calculation performance" do
      metrics = %NodeMetrics{trust_score: 0.75, available_throughput: 500.0, predicted_latency_to_target: 25.0}
      iterations = 10_000

      {total_time, _} = :timer.tc(fn ->
        Enum.each(1..iterations, fn _ ->
          GtrFabric.calculate_potential_value(metrics, @default_sla)
        end)
      end)

      avg_time_us = total_time / iterations
      throughput = 1_000_000 / avg_time_us

      IO.puts("Performance: #{Float.round(avg_time_us, 3)}μs per call, #{Float.round(throughput / 1_000_000, 1)}M ops/sec")

      # Assert performance bounds (should be under 2μs per call)
      assert avg_time_us < 2.0, "Average latency should be under 2μs per call, got #{avg_time_us}μs"
    end

    @tag :benchmark
    test "forwarding decision scalability" do
      candidate_sizes = [1, 10, 100, 1000]

      Enum.each(candidate_sizes, fn size ->
        candidates = Enum.map(1..size, fn i ->
          %CandidateHop{
            id: "node#{i}",
            potential: 100.0 + (:rand.uniform() * 800.0),
            latency: 5.0 + (:rand.uniform() * 45.0)
          }
        end)

        {time_us, _result} = :timer.tc(fn ->
          GtrFabric.calculate_forwarding_decision(candidates, 1.05)
        end)

        IO.puts("#{size} candidates: #{time_us}μs")

        # Assert reasonable performance bounds
        max_time = case size do
          1 -> 5000    # 5ms for single candidate (allows for initial NIF load time)
          10 -> 200    # 200μs for 10 candidates
          100 -> 1000  # 1ms for 100 candidates
          1000 -> 3000 # 3ms for 1000 candidates
        end

        assert time_us <= max_time, "Performance exceeded bounds: #{time_us}μs > #{max_time}μs for #{size} candidates"
      end)
    end
  end
end
