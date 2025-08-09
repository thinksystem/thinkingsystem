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

# GTR Node GenServer implementation
defmodule GtrFabric.Node do
  @moduledoc """
  The GenServer representing a single node in the GTR network.

  Each node maintains its own state including trust score, throughput capacity,
  connections to neighbours, and potential field information for active tasks.
  """

  use GenServer
  require Logger

  alias GtrFabric.TaskCollector

  defstruct [
    :id,
    :trust_score,
    :available_throughput,
    :connections,
    potential_fields: %{}
  ]

  # --- Public API ---
  def start_link(initial_state) do
    GenServer.start_link(__MODULE__, initial_state, name: via_tuple(initial_state.id))
  end

  def forward_packet(node_id, packet) do
    case Registry.lookup(GtrFabric.NodeRegistry, node_id) do
      [{pid, _}] -> GenServer.cast(pid, {:forward_packet, packet})
      [] -> Logger.warning("Attempted to forward packet to non-existent node: %{public: [node_id: node_id]}")
    end
  end

  def update_potential_field(node_id, task_id, potential_field) do
    case Registry.lookup(GtrFabric.NodeRegistry, node_id) do
      [{pid, _}] -> GenServer.cast(pid, {:update_potential_field, task_id, potential_field})
      [] -> Logger.warning("Attempted to update potential field for non-existent node: %{public: [node_id: node_id]}")
    end
  end

  def get_node_state(node_id) do
    case Registry.lookup(GtrFabric.NodeRegistry, node_id) do
      [{pid, _}] -> GenServer.call(pid, :get_state)
      [] -> {:error, :not_found}
    end
  end

  # --- GenServer Callbacks ---
  @impl true
  def init(state) do
    Logger.debug("Starting GTR node: #{state.id}")
    {:ok, %__MODULE__{
      id: state.id,
      trust_score: state.trust_score,
      available_throughput: state.available_throughput,
      connections: state.connections,
      potential_fields: %{}
    }}
  end

  # Main packet forwarding logic
  @impl true
  def handle_cast({:forward_packet, packet}, state) do
    # Add self to the breadcrumb trail
    breadcrumb = %GtrFabric.Breadcrumb{node_id: state.id, timestamp_ms: System.monotonic_time(:millisecond)}
    updated_packet = %{packet | breadcrumb_trail: [breadcrumb | packet.breadcrumb_trail]}


    # Check if this node is the destination
    if state.id == updated_packet.destination_id do
      Logger.info("[#{state.id}] Packet #{updated_packet.id} reached destination")

      # Convert breadcrumb trail format and send to TaskCollector
      trail = Enum.reverse(updated_packet.breadcrumb_trail)
      TaskCollector.collect_packet(updated_packet.task_id, trail)

      {:noreply, state}
    else
      # Not the destination, so decide where to forward
      case make_forwarding_decision(updated_packet, state) do
        "loop" ->
          Logger.warning("[#{state.id}] Packet #{updated_packet.id} detected routing loop - dropping")
          {:noreply, state}

        next_hop ->
          Logger.debug("[#{state.id}] Forwarding packet #{updated_packet.id} to #{next_hop}")
          forward_packet(next_hop, updated_packet)
          {:noreply, state}
      end
    end
  end

  # Handle potential field updates via gossip protocol
  @impl true
  def handle_cast({:update_potential_field, task_id, potential_field}, state) do
    Logger.debug("[#{state.id}] Received potential field update for task #{task_id}")
    new_potential_fields = Map.put(state.potential_fields, task_id, potential_field)
    {:noreply, %{state | potential_fields: new_potential_fields}}
  end

  @impl true
  def handle_call(:get_state, _from, state) do
    {:reply, state, state}
  end

  # --- Private Functions ---

  defp make_forwarding_decision(packet, state) do
    # Get potential field for this task
    potential_field = Map.get(state.potential_fields, packet.task_id, %{})

    # Build candidate hops with current metrics
    candidate_hops = build_candidate_hops(state.connections, potential_field)

    # Call the main GtrFabric module to make the decision
    # The multipath threshold is now sourced from the packet's SLA.
    GtrFabric.calculate_forwarding_decision(candidate_hops, packet.sla.multipath_threshold)
  end

  defp build_candidate_hops(connections, potential_field) do
    Enum.map(connections, fn {neighbour_id, edge_info} ->
      neighbour_potential = Map.get(potential_field, neighbour_id, %{potential: 9999.9})
      %GtrFabric.CandidateHop{
        id: neighbour_id,
        potential: neighbour_potential.potential,
        latency: edge_info.latency_ms
      }
    end)
  end

  # Helper to register the process with a readable name
  defp via_tuple(node_id), do: {:via, Registry, {GtrFabric.NodeRegistry, node_id}}
end
