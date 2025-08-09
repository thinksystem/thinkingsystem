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

# Task collector for analysing completed packet journeys
defmodule GtrFabric.TaskCollector do
  @moduledoc """
  Collects completed packet trails and performs DAG analysis.

  This GenServer accumulates packet trails for each task and triggers
  analysis when all packets for a task have completed their journeys.
  """

  use GenServer
  require Logger

  defstruct [
    trails: %{}, # %{task_id => [packet_trails]}
    slas: %{},   # %{task_id => sla_requirements}
    expected_packets: %{} # %{task_id => expected_count}
  ]

  # Public API
  def start_link(_) do
    GenServer.start_link(__MODULE__, %__MODULE__{}, name: __MODULE__)
  end

  def register_task(task_id, sla, expected_packet_count) do
    GenServer.call(__MODULE__, {:register_task, task_id, sla, expected_packet_count})
  end

  def collect_packet(task_id, packet_trail) do
    GenServer.cast(__MODULE__, {:packet_complete, task_id, packet_trail})
  end

  def get_task_status(task_id) do
    GenServer.call(__MODULE__, {:get_status, task_id})
  end

  # GenServer callbacks
  @impl true
  def init(state) do
    {:ok, state}
  end

  @impl true
  def handle_call({:register_task, task_id, sla, expected_count}, _from, state) do
    new_state = %{state |
      slas: Map.put(state.slas, task_id, sla),
      expected_packets: Map.put(state.expected_packets, task_id, expected_count),
      trails: Map.put(state.trails, task_id, [])
    }
    {:reply, :ok, new_state}
  end

  @impl true
  def handle_call({:get_status, task_id}, _from, state) do
    status = %{
      completed_packets: length(Map.get(state.trails, task_id, [])),
      expected_packets: Map.get(state.expected_packets, task_id, 0),
      sla: Map.get(state.slas, task_id)
    }
    {:reply, status, state}
  end

  @impl true
  def handle_cast({:packet_complete, task_id, trail}, state) do
    current_trails = Map.get(state.trails, task_id, [])
    updated_trails = [trail | current_trails]
    new_state = %{state | trails: Map.put(state.trails, task_id, updated_trails)}

    # Check if task is complete
    expected_count = Map.get(state.expected_packets, task_id, 0)
    if length(updated_trails) >= expected_count and expected_count > 0 do
      perform_analysis(task_id, updated_trails, Map.get(state.slas, task_id), expected_count)
      # Clean up completed task
      final_state = %{new_state |
        trails: Map.delete(new_state.trails, task_id),
        slas: Map.delete(new_state.slas, task_id),
        expected_packets: Map.delete(new_state.expected_packets, task_id)
      }
      {:noreply, final_state}
    else
      {:noreply, new_state}
    end
  end

  # Private functions
  defp perform_analysis(task_id, trails, sla, expected_count) when not is_nil(sla) do
    Logger.info("Analysing task #{task_id} with #{length(trails)} completed packets out of #{expected_count} expected")

    # The `trail` is already in the correct format of `[%Breadcrumb{}]`
    # so no conversion is needed here.

    try do
      report = GtrFabric.analyse_dag(trails, sla, expected_count)
      Logger.info("[Task #{task_id}] Analysis Complete: #{report.analysis_summary}")
      # In a real system, this report would be published to an event bus
      # or used to trigger reward/slashing mechanisms.
      {:ok, report}
    rescue
      e ->
        Logger.error("NIF Error during DAG analysis for task #{task_id}: #{inspect(e)}")
        {:error, :nif_error}
    end
  end

  defp perform_analysis(task_id, _trails, _sla, _expected) do
    Logger.warning("Cannot analyse task #{task_id}: no SLA defined")
    :ok
  end
end
