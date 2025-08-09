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

# Main public API for the GTR system
defmodule GtrFabric do
  @moduledoc """
  The main public API for the GTR (Graph Traversal Routing) system.

  This module provides functions to:
  - Start new routing tasks
  - Create and manage network nodes
  - Monitor system performance
  """

  require Logger

  alias GtrFabric.{Node, NodeSupervisor, TaskCollector, Packet, Protocol, Reputation}

  # Convenience wrappers for NIF functions with error handling
  def calculate_potential_value(node_metrics, sla) do
    # Pre-validate input before calling NIF
    validate_node_metrics!(node_metrics)
    validate_sla!(sla)

    case GtrFabric.CoreWrapper.potential_value(node_metrics, sla) do
      {:ok, v} -> v
      {:error, r} -> raise ArgumentError, inspect(r)
    end
  end

  def calculate_forwarding_decision(candidate_hops, multipath_threshold \\ 1.05) do
    validate_candidate_hops_for_forwarding!(candidate_hops)

    # This is a performance-critical function.
    # We delegate directly to the NIF for speed.
    case GtrFabric.CoreWrapper.forwarding_decision(candidate_hops, multipath_threshold) do
      {:ok, v} -> v
      {:error, r} -> raise ArgumentError, inspect(r)
    end
  end

  def analyse_dag(packet_trails, sla, total_packets_sent) do
    GtrFabric.CoreWrapper.analyse_dag(packet_trails, sla, total_packets_sent)
  end

  # --- Dynamic PoPS API ---

  @doc """
  Takes the current network state and parameters and returns an updated set of parameters.
  This is the core of the dynamic feedback loop.
  """
  def adjust_parameters_for_epoch(current_params, state) do
    Protocol.adjust_parameters_for_epoch(current_params, state)
  end

  @doc "Calculates the percentage of collateral to be slashed based on performance."
  def calculate_slash_percentage(required_performance, actual_performance, params) do
    Protocol.calculate_slash_percentage(
      required_performance,
      actual_performance,
      params
    )
  end

  @doc "Updates a trust score after a successful task."
  def update_trust_score_on_success(score, success_weight) do
    Reputation.update_trust_score_on_success(score, success_weight)
  end

  @doc "Updates a trust score after a failed task, using the dynamic failure weight."
  def update_trust_score_on_failure(score, slash_percentage, params) do
    Reputation.update_trust_score_on_failure(score, slash_percentage, params)
  end

  @doc "Decays a trust score over a period of time based on the dynamic lambda."
  def decay_trust_score_continuously(score, seconds_elapsed, params) do
    Reputation.decay_trust_score_continuously(score, seconds_elapsed, params)
  end

  @doc "Calculates a supplier's offering (stake, price) based on their trust and dynamic parameters."
  def calculate_supplier_offering(trust_score, params) do
    GtrFabric.Strategy.calculate_supplier_offering(trust_score, params)
  end

  @doc "Calculates the utility for a consumer given a supplier's offering."
  def calculate_consumer_utility(offering, trust_score, consumer) do
    GtrFabric.Strategy.calculate_consumer_utility(offering, trust_score, consumer)
  end

  @doc """
  Start a new routing task in the GTR network.

  This function:
  1. Registers the task with the TaskCollector
  2. Generates and propagates the potential field
  3. Injects packets from the consumer node to the destination

  ## Parameters
  - `task_id`: Unique identifier for this routing task
  - `consumer_id`: ID of the node that will inject packets
  - `destination_id`: ID of the target destination node
  - `num_packets`: Number of packets to inject for this task
  - `sla`: SLA requirements (e.g., %{e2e_latency_ms: 100})

  ## Returns
  `:ok` on success, `{:error, reason}` on failure
  """
  def start_task(task_id, consumer_id, destination_id, num_packets, sla \\ nil) do
    Logger.info("Starting task #{task_id} from #{consumer_id} to #{destination_id}")

    # Register task with collector if SLA is provided
    if sla do
      :ok = TaskCollector.register_task(task_id, sla, num_packets)
    end

    # Generate and propagate potential field
    case generate_potential_field(destination_id) do
      {:ok, potential_field} ->
        propagate_potential_field(task_id, potential_field)
        inject_packets(task_id, consumer_id, destination_id, num_packets)
        :ok

      {:error, reason} ->
        Logger.error("Failed to generate potential field: #{inspect(reason)}")
        {:error, reason}
    end
  end

  @doc """
  Create a new GTR node in the network.

  ## Parameters
  - `node_config`: Map containing node configuration
    - `:id` - Unique node identifier
    - `:trust_score` - Trust score (0.0 to 1.0)
    - `:available_throughput` - Available bandwidth
    - `:connections` - Map of neighbour connections

  ## Returns
  `{:ok, pid}` on success, `{:error, reason}` on failure
  """
  def create_node(node_config) do
    case NodeSupervisor.start_node(node_config) do
      {:ok, pid} ->
        Logger.info("Created GTR node: #{node_config.id}")
        {:ok, pid}

      {:error, reason} ->
        Logger.error("Failed to create node #{node_config.id}: #{inspect(reason)}")
        {:error, reason}
    end
  end

  @doc """
  Remove a node from the GTR network.
  """
  def remove_node(node_id) do
    case NodeSupervisor.stop_node(node_id) do
      :ok ->
        Logger.info("Removed GTR node: #{node_id}")
        :ok

      {:error, reason} ->
        Logger.error("Failed to remove node #{node_id}: #{inspect(reason)}")
        {:error, reason}
    end
  end

  @doc """
  Get the current status of a routing task.
  """
  def get_task_status(task_id) do
    TaskCollector.get_task_status(task_id)
  end

  @doc """
  Get the current state of a network node.
  """
  def get_node_state(node_id) do
    Node.get_node_state(node_id)
  end

  @doc """
  Issues a formal, cryptographically signed Verifiable Credential for a trust score.
  This bridges the GTR reputation system with the Steel identity system.
  """
  def issue_trust_score_credential(
        subject_did,
        trust_score_struct,
        performance_ledger,
        issuer_token
      ) do
    # Extract the raw score value
    trust_score = trust_score_struct.value

    # Create a simple performance summary from the ledger stats
    performance_summary = case performance_ledger do
      %GtrFabric.PerformanceLedger{} = ledger -> GtrFabric.PerformanceLedger.calculate_stats(ledger)
      map when is_map(map) -> map
    end

    # Call the NIF
    case GtrFabric.CoreWrapper.create_trust_vc(subject_did, trust_score, convert_performance_to_strings(performance_summary), issuer_token) do
      {:ok, %{__struct__: _} = vc} -> {:ok, vc}
      {:ok, vc} -> {:ok, vc}
      {:error, r} -> {:error, r}
    end
  end

  # Helper function to convert performance summary values to strings
  defp convert_performance_to_strings(performance_summary) when is_map(performance_summary) do
    performance_summary
    |> Enum.map(fn {k, v} -> {to_string(k), to_string(v)} end)
    |> Map.new()
  end

  # --- Private Functions ---

  # Generate potential field for a destination (simplified version)
  # In a real system, this would use a distributed algorithm
  defp generate_potential_field(destination_id) do
    # This is a simplified potential field generation
    # In reality, this would involve complex graph algorithms and distributed computation

    # Get all active nodes from the registry
    active_nodes = Registry.select(GtrFabric.NodeRegistry, [{{:"$1", :"$2", :"$3"}, [], [:"$1"]}])

    if destination_id not in active_nodes do
      {:error, :destination_not_found}
    else
      # Simple distance-based potential field (in real system, use Dijkstra/gossip protocol)
      potential_field =
        active_nodes
        |> Enum.reduce(%{}, fn node_id, acc ->
          potential = if node_id == destination_id do
            0.0  # Destination has zero potential
          else
            # Simplified: use hash-based "distance" for demo
            abs(:erlang.phash2({node_id, destination_id})) / 1000.0
          end
          Map.put(acc, node_id, %{potential: potential})
        end)

      {:ok, potential_field}
    end
  end

  # Propagate potential field to all nodes via gossip protocol
  defp propagate_potential_field(task_id, potential_field) do
    Registry.dispatch(GtrFabric.NodeRegistry, :all, fn entries ->
      for {pid, _} <- entries do
        GenServer.cast(pid, {:update_potential_field, task_id, potential_field})
      end
    end)
  end

  # Inject initial packets from the consumer node
  defp inject_packets(task_id, consumer_id, destination_id, num_packets) do
    for i <- 1..num_packets do
      packet = %Packet{
        id: i,
        task_id: task_id,
        destination_id: destination_id,
        payload: "packet_data_#{i}"
      }

      # Small delay between packet injections to simulate realistic traffic
      if i > 1, do: Process.sleep(1)

      Node.forward_packet(consumer_id, packet)
    end
  end

  # Private validation functions

  defp validate_node_metrics!(metrics) when is_nil(metrics) do
    raise ArgumentError, "NodeMetrics cannot be nil"
  end

  defp validate_node_metrics!(%GtrFabric.NodeMetrics{} = metrics) do
    validate_finite_number!(metrics.trust_score, "trust_score")
    validate_finite_number!(metrics.available_throughput, "available_throughput")
    validate_finite_number!(metrics.predicted_latency_to_target, "predicted_latency_to_target")
  end

  defp validate_node_metrics!(_) do
    raise ArgumentError, "Invalid NodeMetrics struct"
  end

  defp validate_sla!(sla) when is_nil(sla) do
    raise ArgumentError, "SLA cannot be nil"
  end

  defp validate_sla!(%GtrFabric.SLA{} = _sla) do
    # SLA fields are validated by the struct definition
    :ok
  end

  defp validate_sla!(_) do
    raise ArgumentError, "Invalid SLA struct"
  end

  defp validate_finite_number!(value, field_name) when is_number(value) do
    cond do
      not is_finite(value) ->
        raise ArgumentError, "#{field_name} must be finite (not infinity or NaN), got: #{inspect(value)}"

      # Additional specific checks can be added here
      true -> :ok
    end
  end

  defp validate_finite_number!(value, field_name) when value in [:infinity, :neg_infinity, :nan] do
    raise ArgumentError, "#{field_name} must be finite (not infinity or NaN), got: #{inspect(value)}"
  end

  defp validate_finite_number!(value, field_name) do
    raise ArgumentError, "#{field_name} must be a number, got: #{inspect(value)}"
  end

  defp is_finite(value) when is_float(value) do
    value != :infinity and value != :neg_infinity and value == value  # NaN != NaN
  end

  defp is_finite(value) when is_integer(value), do: true
  defp is_finite(_), do: false

  defp validate_candidate_hops_for_forwarding!(hops) when not is_list(hops) do
    raise ArgumentError, "Candidate hops must be a list"
  end

  defp validate_candidate_hops_for_forwarding!(hops) do
    Enum.each(hops, fn
      %GtrFabric.CandidateHop{potential: p, latency: l} ->
        if not (is_float(p) and is_finite(p)) do
          raise ArgumentError, "CandidateHop potential must be a finite float, got: #{inspect(p)}"
        end

        if not (is_float(l) and is_finite(l)) do
          raise ArgumentError, "CandidateHop latency must be a finite float, got: #{inspect(l)}"
        end

      other ->
        raise ArgumentError,
              "All elements in candidate_hops must be %GtrFabric.CandidateHop{} structs, got: #{inspect(other)}"
    end)
  end

  def spawn_demo_nodes(count \\ 2) do
    ids = for i <- 1..count do
      id = "gw-" <> Integer.to_string(i)
      trust = :erlang.phash2(id, 10_000) / 10_000
      {:ok, _} = create_node(%{id: id, trust_score: trust, available_throughput: 1000, connections: %{}})
      id
    end

    Enum.each(ids, fn id ->
      {:ok, state} = case get_node_state(id) do
        %GtrFabric.Node{} = s -> {:ok, s}
        other -> {:ok, other}
      end
      neighbours = ring_neighbours(ids, id)
      updated = %{state | connections: Map.new(neighbours, fn n -> {n, %{latency_ms: synthetic_latency(id, n)}} end)}
      case Registry.lookup(GtrFabric.NodeRegistry, id) do
        [{pid, _}] when is_pid(pid) -> :sys.replace_state(pid, fn _ -> updated end)
        _ -> Logger.warning("Could not find node process for #{id} to update connections")
      end
    end)

    maybe_start_trust_mutator()
    ids
  end

  def refresh_demo_nodes(new_count) do
    # remove existing
    Registry.select(GtrFabric.NodeRegistry, [{{:"$1", :_, :_}, [], [:"$1"]}])
    |> Enum.each(&remove_node/1)
    spawn_demo_nodes(new_count)
  end

  defp maybe_start_trust_mutator do
    if Process.whereis(:gtr_trust_mutator) == nil do
      pid = spawn_link(fn -> trust_mutator_loop() end)
      Process.register(pid, :gtr_trust_mutator)
    end
  end

  defp trust_mutator_loop do
    receive do
    after 5_000 ->
      Registry.select(GtrFabric.NodeRegistry, [{{:"$1", :_, :_}, [], [:"$1"]}])
      |> Enum.each(fn id ->
        case get_node_state(id) do
          %{trust_score: t} = state when is_number(t) ->
            delta = (:rand.uniform() - 0.5) * 0.04
            new_t = min(1.0, max(0.0, t + delta))
            case Registry.lookup(GtrFabric.NodeRegistry, id) do
              [{pid, _}] -> :sys.replace_state(pid, fn _ -> %{state | trust_score: new_t} end)
              _ -> :ok
            end
          _ -> :ok
        end
      end)
    end
    trust_mutator_loop()
  end

  defp ring_neighbours(ids, id) do
    idx = Enum.find_index(ids, & &1 == id)
    left = Enum.at(ids, rem(idx - 1 + length(ids), length(ids)))
    right = Enum.at(ids, rem(idx + 1, length(ids)))
    Enum.uniq([left, right])
  end

  defp synthetic_latency(a, b) do
    # Stable pseudo latency 50-150ms
    base = abs(:erlang.phash2({a, b}, 100)) + 50
    base
  end

  def providers_snapshot do
    params = GtrFabric.DynamicParameters.new()
    consumer = %GtrFabric.ConsumerFactors{risk_aversion: 0.3, budget: 2_000, cost_of_failure: 5_000.0}
    Registry.select(GtrFabric.NodeRegistry, [{{:"$1", :_, :_}, [], [:"$1"]}])
    |> Enum.map(fn id ->
      case get_node_state(id) do
        %{trust_score: trust} = state ->
          trust_struct = %GtrFabric.TrustScore{value: trust, last_updated_ts: 0}
          offering_tuple = GtrFabric.CoreWrapper.supplier_offering(trust_struct, params)
          {price, collateral} = case offering_tuple do
            {:ok, {p, c}} -> {p, c}
            _ -> {0, 0}
          end
          util = case GtrFabric.CoreWrapper.consumer_utility(%GtrFabric.PublishedOffering{staked_collateral: collateral, price_per_call: price}, trust_struct, consumer) do
            {:ok, v} -> v
            _ -> 0.0
          end
          %{id: id, trust: trust, offering: %{price_per_call: price, staked_collateral: collateral}, utility: util, th: state.available_throughput}
        _ -> %{id: id, trust: 0.5}
      end
    end)
  end
end
