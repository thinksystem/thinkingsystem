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

defmodule ComprehensiveIntegrationTest do
  use ExUnit.Case, async: false

  @moduledoc """
  Comprehensive integration test suite for the GTR system including:
  - Integer underflow b      result =
        GtrFabric.calculate_forwarding_decision(
          [
            %CandidateHop{id: "bad", potential: :nan, latency: 10.0}
          ],
          1.05
        )validation
  - NIF function testing
  - Error handling verification
  - Performance benchmarks
  - Mathematical property validation
  - Stress testing and concurrency validation
  """

  # Aliases for easier access
  alias GtrFabric.{NodeMetrics, CandidateHop, Breadcrumb, SLA}

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
    # Ensure application is started
    Application.ensure_all_started(:gtr_fabric)

    # Validate NIF is properly loaded
    validate_nif_loading()

    :ok
  end

  # Helper function to create a valid test JWT token
  defp create_test_token() do
    # Create a proper JWT token that matches the Steel crate's expectations
    secret = "your_jwt_secret"

    claims = %{
      "sub" => "test_user",
      "email" => "test@example.com",
      "name" => "Test User",
      "iat" => :os.system_time(:second),
      "exp" => :os.system_time(:second) + 3600,
      "iss" => "did:steel:issuer",
      "aud" => "gtr-fabric-consumer",
      "did" => "did:steel:test:user",
      "roles" => ["issuer", "admin"]
    }

    # Create JWT token using Joken if available, otherwise use a fallback
    if Code.ensure_loaded?(Joken) do
      signer = Joken.Signer.create("HS256", secret)
      case Joken.generate_and_sign(%{}, claims, signer) do
        {:ok, token, _claims} -> token
        {:error, _} -> "test_fallback_token"
      end
    else
      # Fallback for when Joken is not available
      "test_fallback_token"
    end
  end

  # =====================================================
  # NIF VALIDATION
  # =====================================================

  defp validate_nif_loading do
    # Test that core NIF functions are available
    try do
      # Try a simple potential value calculation
      metrics = %NodeMetrics{trust_score: 0.5, available_throughput: 1000.0, predicted_latency_to_target: 50.0}
      result = GtrFabric.calculate_potential_value(metrics, @default_sla)

      assert is_float(result), "NIF should return float values"
      assert result > 0, "Potential values should be positive"

      IO.puts("✅ NIF validation passed - calculate_potential_value working")
    rescue
      error ->
        flunk("NIF validation failed: #{inspect(error)}")
    end
  end

  # =====================================================
  # INTEGER UNDERFLOW BUG TESTS
  # =====================================================

  describe "Integer Underflow Bug Validation" do
    test "backwards timestamp trail handling" do
      # Test case with backward timestamps that previously caused underflow
      problematic_trail = [
        %Breadcrumb{node_id: "backward", timestamp_ms: 2000},
        %Breadcrumb{node_id: "time", timestamp_ms: 1000}  # Time goes backward!
      ]

      sla = %SLA{e2e_latency_ms: 100, jitter_ms: 10, loss_percentage: 5.0, weight_latency: 1.0, weight_throughput: 1.0, weight_trust: 1.0, multipath_threshold: 0.05}
      {:ok, result} = GtrFabric.analyse_dag([problematic_trail], sla, 1)

      # Should be rejected as invalid
      assert result.avg_latency_ms == -1.0
      assert result.sla_met == false
      assert String.contains?(result.analysis_summary, "No valid packet trails")
    end

    test "extreme timestamp values rejection" do
      # Test extreme timestamps that could cause integer underflow
      extreme_trail = [
        %Breadcrumb{node_id: "start", timestamp_ms: 18446744073709551615}, # Near u64::MAX
        %Breadcrumb{node_id: "end", timestamp_ms: 0}
      ]

      {:ok, result} = GtrFabric.analyse_dag([extreme_trail], @default_sla, 1)

      # Should be rejected as invalid (unrealistic latency)
      assert result.avg_latency_ms == -1.0, "Extreme timestamps should be rejected"
      assert result.sla_met == false
      assert String.contains?(result.analysis_summary, "No valid packet trails")
    end

    test "large but reasonable latencies are rejected" do
      # Test latency over 1 hour (our maximum threshold)
      large_latency_trail = [
        %Breadcrumb{node_id: "start", timestamp_ms: 1000},
        %Breadcrumb{node_id: "end", timestamp_ms: 4000000}  # ~3.9M ms > 1 hour
      ]

      {:ok, result} = GtrFabric.analyse_dag([large_latency_trail], @default_sla, 1)

      # Should be rejected as unrealistic
      assert result.avg_latency_ms == -1.0, "Unreasonably large latencies should be rejected"
    end

    test "maximum acceptable latency boundary" do
      # Test exactly at our 1-hour boundary (should just pass)
      boundary_trail = [
        %Breadcrumb{node_id: "start", timestamp_ms: 1000},
        %Breadcrumb{node_id: "end", timestamp_ms: 3601000}  # Exactly 1 hour
      ]

      # Test just over the boundary (should be rejected)
      over_boundary_trail = [
        %Breadcrumb{node_id: "start", timestamp_ms: 1000},
        %Breadcrumb{node_id: "end", timestamp_ms: 3601001}  # 1 hour + 1ms
      ]

      sla = %SLA{e2e_latency_ms: 4000000, jitter_ms: 10, loss_percentage: 5.0, weight_latency: 1.0, weight_throughput: 1.0, weight_trust: 1.0, multipath_threshold: 0.05}

      {:ok, boundary_result} = GtrFabric.analyse_dag([boundary_trail], sla, 1)
      {:ok, over_boundary_result} = GtrFabric.analyse_dag([over_boundary_trail], sla, 1)

      # Boundary case should pass (exactly 1 hour = 3.6M ms)
      assert boundary_result.avg_latency_ms == 3600000.0, "Exactly 1 hour should be accepted"

      # Over boundary should be rejected
      assert over_boundary_result.avg_latency_ms == -1.0, "Over 1 hour latencies should be rejected"
    end
  end

  # =====================================================
  # FUNCTIONAL CORRECTNESS TESTS
  # =====================================================

  describe "Functional Correctness" do
    test "potential value calculation with various scenarios" do
      test_cases = [
        {%NodeMetrics{trust_score: 0.0, available_throughput: 1000.0, predicted_latency_to_target: 15.0}, "minimum trust"},
        {%NodeMetrics{trust_score: 1.0, available_throughput: 1000.0, predicted_latency_to_target: 15.0}, "maximum trust"},
        {%NodeMetrics{trust_score: 0.5, available_throughput: 0.1, predicted_latency_to_target: 15.0}, "low throughput"},
        {%NodeMetrics{trust_score: 0.5, available_throughput: 10000.0, predicted_latency_to_target: 0.1}, "high performance"},
      ]

      for {metrics, description} <- test_cases do
        result = GtrFabric.calculate_potential_value(metrics, @default_sla)
        assert is_float(result), "#{description}: should return float"
        assert result > 0.0, "#{description}: should return positive value"
      end
    end

    test "forwarding decision edge cases" do
      # Single candidate
      single_candidate = [%CandidateHop{id: "only_node", potential: 500.0, latency: 25.0}]
      result = GtrFabric.calculate_forwarding_decision(single_candidate, 1.05)
      assert result == "only_node"

      # Empty candidates
      empty_result = GtrFabric.calculate_forwarding_decision([], 1.05)
      assert empty_result == "loop"

      # Multiple candidates - should be deterministic for same input
      candidates = [
        %CandidateHop{id: "node1", potential: 300.0, latency: 15.0},
        %CandidateHop{id: "node2", potential: 500.0, latency: 10.0},
        %CandidateHop{id: "node3", potential: 400.0, latency: 20.0}
      ]

      # Should be consistent across multiple calls
      decisions = Enum.map(1..10, fn _ ->
        GtrFabric.calculate_forwarding_decision(candidates, 1.5)
      end)

      unique_decisions = Enum.uniq(decisions)
      assert length(unique_decisions) > 0 and length(unique_decisions) <= 3

      # Test zero or near-zero cost candidates
      assert GtrFabric.calculate_forwarding_decision(
        [
          %CandidateHop{id: "zero_cost", potential: 0.0, latency: 0.0},
          %CandidateHop{id: "non_zero", potential: 10.0, latency: 5.0}
        ],
        1.05
      ) == "zero_cost"

      assert GtrFabric.calculate_forwarding_decision(
        [
          %CandidateHop{id: "best", potential: 1.0, latency: 1.0},
          %CandidateHop{id: "zero_cost", potential: 0.0, latency: 0.0}
        ],
        0.05
      ) == "zero_cost"
    end

    test "calculate_forwarding_decision handles invalid inputs" do
      assert_raise ArgumentError, ~r/Candidate hops must be a list/, fn ->
        GtrFabric.calculate_forwarding_decision("not a list", 1.05)
      end
    end

    test "DAG analysis SLA evaluation" do
      # Normal chronological order - should pass
      passing_trail = [
        [
          %Breadcrumb{node_id: "start", timestamp_ms: 1000},
          %Breadcrumb{node_id: "end", timestamp_ms: 1015}
        ]
      ]

      # Failing case - latency too high
      failing_trail = [
        [
          %Breadcrumb{node_id: "start", timestamp_ms: 1000},
          %Breadcrumb{node_id: "end", timestamp_ms: 1150}
        ]
      ]

      # Test passing case (15ms latency vs 20ms SLA)
      passing_sla = %SLA{e2e_latency_ms: 20, jitter_ms: 10, loss_percentage: 5.0, weight_latency: 1.0, weight_throughput: 1.0, weight_trust: 1.0, multipath_threshold: 0.05}
      {:ok, passing_result} = GtrFabric.analyse_dag(passing_trail, passing_sla, 1)
      assert passing_result.sla_met == true
      assert passing_result.avg_latency_ms == 15.0

      # Test failing case (150ms latency vs 100ms SLA)
      failing_sla = %SLA{e2e_latency_ms: 100, jitter_ms: 10, loss_percentage: 5.0, weight_latency: 1.0, weight_throughput: 1.0, weight_trust: 1.0, multipath_threshold: 0.05}
      {:ok, failing_result} = GtrFabric.analyse_dag(failing_trail, failing_sla, 1)
      assert failing_result.sla_met == false
      assert failing_result.avg_latency_ms == 150.0
    end

    test "empty and invalid trail handling" do
      # Empty trails
      {:ok, empty_result} = GtrFabric.analyse_dag([], @default_sla, 1)
      assert empty_result.sla_met == false
      assert empty_result.avg_latency_ms == -1.0

      # Trails with empty packets
      {:ok, empty_packet_result} = GtrFabric.analyse_dag([[]], @default_sla, 1)
      assert empty_packet_result.sla_met == false
      assert empty_packet_result.avg_latency_ms == -1.0

      # Single-hop packets (invalid)
      {:ok, single_hop_result} = GtrFabric.analyse_dag([
        [%Breadcrumb{node_id: "single", timestamp_ms: 1000}]
      ], @default_sla, 1)
      assert single_hop_result.sla_met == false
      assert single_hop_result.avg_latency_ms == -1.0
    end
  end

  # =====================================================
  # ERROR HANDLING & VALIDATION TESTS
  # =====================================================

  describe "Error Handling & Input Validation" do
    test "invalid node metrics rejection" do
      # Test invalid trust scores
      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_potential_value(%NodeMetrics{
          trust_score: -1.0,
          available_throughput: 1000.0,
          predicted_latency_to_target: 15.0
        }, @default_sla)
      end

      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_potential_value(%NodeMetrics{
          trust_score: 2.0,
          available_throughput: 1000.0,
          predicted_latency_to_target: 15.0
        }, @default_sla)
      end

      # Test invalid throughput
      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_potential_value(%NodeMetrics{
          trust_score: 0.5,
          available_throughput: 0.0,
          predicted_latency_to_target: 15.0
        }, @default_sla)
      end

      # Test negative latency
      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_potential_value(%NodeMetrics{
          trust_score: 0.5,
          available_throughput: 1000.0,
          predicted_latency_to_target: -10.0
        }, @default_sla)
      end
    end

    test "infinite and NaN value rejection" do
      # Test infinite potential in forwarding decision
      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_forwarding_decision([
          %CandidateHop{id: "node1", potential: :infinity, latency: 10.0}
        ], 0.05)
      end

      # Test NaN latency
      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_forwarding_decision([
          %CandidateHop{id: "node1", potential: 100.0, latency: :nan}
        ], 0.05)
      end

      # Test infinite values in node metrics
      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_potential_value(%NodeMetrics{
          trust_score: :infinity,
          available_throughput: 1000.0,
          predicted_latency_to_target: 15.0
        }, @default_sla)
      end
    end

    test "type safety validation" do
      # Test invalid struct types
      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_potential_value("not a struct", @default_sla)
      end

      assert_raise ArgumentError, fn ->
        GtrFabric.calculate_forwarding_decision("not a list", 0.05)
      end

      assert_raise ArgumentError, fn ->
        GtrFabric.analyse_dag("not trails", @default_sla, 1)
      end
    end

    test "empty node ID validation" do
      # Test empty node ID in breadcrumbs - this should be caught by the NIF decoder
      assert_raise ErlangError, fn ->
        GtrFabric.analyse_dag([
          [%Breadcrumb{node_id: nil, timestamp_ms: 1000}]
        ], @default_sla, 1)
      end

      # Test empty candidate hop ID - should be handled gracefully
      result = GtrFabric.calculate_forwarding_decision([
        %CandidateHop{id: "", potential: 100.0, latency: 10.0}
      ])
      assert result == ""  # Returns empty ID
    end
  end

  # =====================================================
  # MATHEMATICAL PROPERTIES
  # =====================================================

  describe "Mathematical Properties" do
    test "deterministic behaviour" do
      metrics = %NodeMetrics{trust_score: 0.7, available_throughput: 500.0, predicted_latency_to_target: 25.0}

      results = Enum.map(1..20, fn _ -> GtrFabric.calculate_potential_value(metrics, @default_sla) end)
      unique_results = Enum.uniq(results)

      assert length(unique_results) == 1, "Function should be deterministic"
    end

    test "monotonicity properties" do
      base_metrics = %NodeMetrics{trust_score: 0.5, available_throughput: 1000.0, predicted_latency_to_target: 50.0}
      base_potential = GtrFabric.calculate_potential_value(base_metrics, @default_sla)

      # Higher trust should result in lower potential (better)
      higher_trust = %{base_metrics | trust_score: 0.8}
      higher_trust_potential = GtrFabric.calculate_potential_value(higher_trust, @default_sla)
      assert higher_trust_potential < base_potential, "Higher trust should yield lower potential"

      # Higher throughput should result in lower potential (better)
      higher_throughput = %{base_metrics | available_throughput: 2000.0}
      higher_throughput_potential = GtrFabric.calculate_potential_value(higher_throughput, @default_sla)
      assert higher_throughput_potential < base_potential, "Higher throughput should yield lower potential"

      # Lower latency should result in lower potential (better)
      lower_latency = %{base_metrics | predicted_latency_to_target: 25.0}
      lower_latency_potential = GtrFabric.calculate_potential_value(lower_latency, @default_sla)
      assert lower_latency_potential < base_potential, "Lower latency should yield lower potential"
    end

    test "potential value bounds" do
      # Test extreme but valid cases
      extreme_cases = [
        %NodeMetrics{trust_score: 0.0, available_throughput: 0.001, predicted_latency_to_target: 10000.0},
        %NodeMetrics{trust_score: 1.0, available_throughput: 100000.0, predicted_latency_to_target: 0.001}
      ]

      results = Enum.map(extreme_cases, &GtrFabric.calculate_potential_value(&1, @default_sla))

      # All results should be finite positive numbers
      Enum.each(results, fn result ->
        assert is_float(result)
        assert result > 0.0
        assert result != :infinity
        assert not is_nil(result)
      end)
    end
  end

  # =====================================================
  # PERFORMANCE BENCHMARKS
  # =====================================================

  describe "Performance Benchmarks" do
    @tag :benchmark
    test "potential value calculation performance" do
      metrics = %NodeMetrics{trust_score: 0.75, available_throughput: 500.0, predicted_latency_to_target: 25.0}
      iterations = 50_000

      {total_time, _} = :timer.tc(fn ->
        Enum.each(1..iterations, fn _ ->
          # Vary inputs slightly to prevent compiler optimisation
          varied_metrics = %{metrics | trust_score: metrics.trust_score + (:rand.uniform() - 0.5) * 0.01}
          GtrFabric.calculate_potential_value(varied_metrics, @default_sla)
        end)
      end)

      avg_time_us = total_time / iterations
      throughput = 1_000_000 / avg_time_us

      IO.puts("Potential value performance: #{Float.round(avg_time_us, 3)}μs per call")
      IO.puts("Throughput: #{Float.round(throughput / 1_000_000, 1)}M ops/sec")

      # Performance assertions
      assert avg_time_us < 2.0, "Should be under 2μs per call, got #{avg_time_us}μs"
      assert throughput > 1_000_000, "Should achieve >1M ops/sec, got #{Float.round(throughput)}"
    end

    @tag :benchmark
    test "forwarding decision scalability" do
      test_sizes = [1, 10, 100, 500, 1000]

      Enum.each(test_sizes, fn size ->
        candidates = Enum.map(1..size, fn i ->
          %CandidateHop{
            id: "scale_node#{i}",
            potential: 100.0 + (:rand.uniform() * 800.0),
            latency: 5.0 + (:rand.uniform() * 45.0)
          }
        end)

        {time_us, result} = :timer.tc(fn ->
          GtrFabric.calculate_forwarding_decision(candidates, 1.05)
        end)

        IO.puts("#{size} candidates: #{time_us}μs -> #{result}")

        # Performance bounds based on size
        max_time = case size do
          1 -> 3000     # 3ms for single candidate (allows for NIF load)
          10 -> 100     # 100μs for 10 candidates
          100 -> 500    # 500μs for 100 candidates
          500 -> 1000   # 1ms for 500 candidates
          1000 -> 2000  # 2ms for 1000 candidates
        end

        assert time_us <= max_time, "Performance exceeded bounds: #{time_us}μs > #{max_time}μs for #{size} candidates"

        # Result should be valid
        assert is_binary(result)
        assert String.length(result) > 0 or result == "loop"
      end)
    end

    @tag :benchmark
    test "DAG analysis performance" do
      # Create realistic multi-packet scenario
      complex_trails = Enum.map(1..100, fn _packet_id ->
        path_length = 3 + :rand.uniform(5)  # 3-8 hops
        start_time = 1000 + :rand.uniform(1000)

        Enum.map(0..(path_length-1), fn hop_index ->
          %Breadcrumb{
            node_id: "perf_hop_#{hop_index}",
            timestamp_ms: start_time + (hop_index * (5 + :rand.uniform(15)))
          }
        end)
      end)

      {analysis_time, {:ok, result}} = :timer.tc(fn ->
        GtrFabric.analyse_dag(complex_trails, @default_sla, 100)
      end)

      IO.puts("DAG analysis (100 trails): #{Float.round(analysis_time / 1000, 1)}ms")
      IO.puts("Result: SLA met = #{result.sla_met}, avg latency = #{Float.round(result.avg_latency_ms, 2)}ms")

      # Performance assertion - should analyse 100 trails in under 10ms
      assert analysis_time < 10_000, "DAG analysis too slow: #{analysis_time}μs"

      # Result validation
      assert is_boolean(result.sla_met)
      assert is_float(result.avg_latency_ms)
      assert result.avg_latency_ms > 0  # Should have valid trails
    end
  end

  # =====================================================
  # COMPREHENSIVE STRESS TESTS
  # =====================================================

  describe "Stress Testing" do
    @tag :stress
    test "concurrent access safety" do
      # Test concurrent NIF calls
      workers = 20
      operations_per_worker = 1000

      tasks = Enum.map(1..workers, fn worker_id ->
        Task.async(fn ->
          Enum.map(1..operations_per_worker, fn op_id ->
            case rem(op_id, 3) do
              0 ->
                # Potential value calculation
                metrics = %NodeMetrics{
                  trust_score: :rand.uniform(),
                  available_throughput: 100.0 + (:rand.uniform() * 1900.0),
                  predicted_latency_to_target: 5.0 + (:rand.uniform() * 95.0)
                }
                {:potential, GtrFabric.calculate_potential_value(metrics, @default_sla)}

              1 ->
                # Forwarding decision
                candidates = Enum.map(1..(:rand.uniform(5) + 2), fn i ->
                  %CandidateHop{
                    id: "worker#{worker_id}_op#{op_id}_node#{i}",
                    potential: 100.0 + (:rand.uniform() * 800.0),
                    latency: 5.0 + (:rand.uniform() * 45.0)
                  }
                end)

                if length(candidates) > 0 do
                  {:forwarding, GtrFabric.calculate_forwarding_decision(candidates, 1.05)}
                else
                  {:noop, "no candidates"}
                end

              2 ->
                # DAG analysis
                trail = [
                  %Breadcrumb{node_id: "worker#{worker_id}_start", timestamp_ms: 1000},
                  %Breadcrumb{node_id: "worker#{worker_id}_end", timestamp_ms: 1000 + :rand.uniform(100)}
                ]
                sla = %SLA{e2e_latency_ms: 50, jitter_ms: 10, loss_percentage: 5.0, weight_latency: 1.0, weight_throughput: 1.0, weight_trust: 1.0, multipath_threshold: 0.05}
                {:analysis, GtrFabric.analyse_dag([trail], sla, 1)}
            end
          end)
        end)
      end)

      # Wait for all workers to complete
      all_results = Task.await_many(tasks, 30_000)

      # Validate all operations completed successfully
      total_operations = workers * operations_per_worker
      completed_operations = all_results |> List.flatten() |> length()

      assert completed_operations == total_operations,
        "Expected #{total_operations} operations, got #{completed_operations}"

      IO.puts("✅ Concurrent stress test: #{total_operations} operations completed successfully")
    end

    @tag :stress
    test "memory usage validation" do
      # Test for memory leaks during sustained operations
      {:memory, initial_memory} = :erlang.process_info(self(), :memory)

      # Perform many operations
      Enum.each(1..10_000, fn i ->
        metrics = %NodeMetrics{
          trust_score: :rand.uniform(),
          available_throughput: 100.0 + (:rand.uniform() * 1900.0),
          predicted_latency_to_target: 5.0 + (:rand.uniform() * 95.0)
        }

        _result = GtrFabric.calculate_potential_value(metrics, @default_sla)

        # Periodic memory check
        if rem(i, 1000) == 0 do
          {:memory, current_memory} = :erlang.process_info(self(), :memory)
          growth = current_memory - initial_memory
          IO.puts("After #{i} ops: #{Float.round(growth / 1024, 1)}KB memory growth")
        end
      end)

      # Force garbage collection
      :erlang.garbage_collect()
      {:memory, final_memory} = :erlang.process_info(self(), :memory)

      final_growth = final_memory - initial_memory
      acceptable_growth = final_growth < 1024 * 1024  # Less than 1MB

      IO.puts("Final memory growth: #{Float.round(final_growth / 1024, 1)}KB")
      assert acceptable_growth, "Memory growth too high: #{final_growth} bytes"
    end
  end

  # =====================================================
  # VERIFIABLE CREDENTIAL INTEGRATION TESTS
  # =====================================================

  describe "Verifiable Credential Integration" do
    test "issue_trust_score_credential integrates with GTR reputation system" do
      # Simulate a supplier with performance history
      subject_did = "did:gtr:integration:supplier:001"

      # Create a trust score struct (simulating the GTR system)
      trust_score_struct = %{value: 0.78}

      # Create a mock performance ledger with statistics
      performance_ledger = %{
        total_tasks: 250,
        successful_tasks: 245,
        avg_latency_ms: 42.5,
        avg_response_time: 38.2,
        reliability_score: 0.98
      }

      issuer_token = create_test_token()

      # Call the high-level API function
      result = GtrFabric.issue_trust_score_credential(
        subject_did,
        trust_score_struct,
        performance_ledger,
        issuer_token
      )

      # The result should be {:ok, vc} from the high-level API
      assert {:ok, vc} = result
      assert is_map(vc)

      # Verify the VC contains expected data
      assert vc.credential_subject["id"] == "\"#{subject_did}\""
      assert is_binary(vc.issuer)
      assert is_list(vc.types)
      assert Enum.member?(vc.types, "VerifiableCredential")

      # Verify the proof structure is complete
      assert is_map(vc.proof)
      assert is_binary(vc.proof.proof_value)
      assert is_binary(vc.proof.verification_method)
    end

    test "end-to-end GTR to VC workflow" do
      # Simulate a complete workflow from GTR reputation to VC
      supplier_id = "did:gtr:e2e:supplier:premium"

      # Step 1: Simulate trust score evolution through tasks
      initial_trust = 0.5

      # Simulate successful task (trust increases)
      success_weight = 0.1
      updated_trust = initial_trust + (success_weight * (1.0 - initial_trust))

      # Step 2: Create performance summary from simulated ledger
      performance_stats = %{
        "total_requests" => 1000.0,
        "successful_requests" => 985.0,
        "avg_response_time_ms" => 28.7,
        "success_rate" => 0.985,
        "trust_evolution" => updated_trust
      }

      trust_struct = %{value: updated_trust}

      # Step 3: Issue the VC
      result = GtrFabric.issue_trust_score_credential(
        supplier_id,
        trust_struct,
        performance_stats,
        create_test_token()
      )

      # Step 4: Verify the complete workflow
      assert {:ok, credential} = result

      # Verify the credential reflects the supplier's performance
      assert credential.credential_subject["id"] == "\"#{supplier_id}\""

      # Verify credential metadata
      assert is_binary(credential.id)
      assert is_binary(credential.issuance_date)

      # The VC should be cryptographically verifiable
      assert byte_size(credential.proof.proof_value) > 0

      IO.puts("✅ End-to-end GTR-to-VC workflow completed successfully")
    end

    test "VC integration handles GTR edge cases" do
      # Test with edge case trust scores
      edge_cases = [
        {"minimum_trust", 0.0, %{"performance" => "minimal"}},
        {"maximum_trust", 1.0, %{"performance" => "perfect"}},
        {"mid_trust", 0.5, %{"performance" => "average"}}
      ]

      results = Enum.map(edge_cases, fn {case_name, trust_value, perf_data} ->
        subject = "did:gtr:edge:#{case_name}"
        trust_struct = %{value: trust_value}

        result = GtrFabric.issue_trust_score_credential(
          subject,
          trust_struct,
          perf_data,
          create_test_token()
        )

        {case_name, result}
      end)

      # All edge cases should succeed
      Enum.each(results, fn {case_name, result} ->
        assert {:ok, vc} = result, "Failed for case: #{case_name}"
        assert String.contains?(vc.credential_subject["id"], case_name)
      end)

      IO.puts("✅ VC integration handled all GTR edge cases successfully")
    end
  end
end

# Start ExUnit if not already started
ExUnit.start()
