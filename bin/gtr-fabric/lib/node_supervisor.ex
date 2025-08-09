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

# Dynamic supervisor to manage GTR node processes
defmodule GtrFabric.NodeSupervisor do
  @moduledoc """
  A dynamic supervisor to manage the lifecycle of potentially millions of node processes.
  
  Each node in the GTR network runs as a separate GenServer process under this supervisor,
  providing fault tolerance and isolation.
  """
  
  use DynamicSupervisor

  def start_link(init_arg) do
    DynamicSupervisor.start_link(__MODULE__, init_arg, name: __MODULE__)
  end

  @impl true
  def init(_init_arg) do
    DynamicSupervisor.init(strategy: :one_for_one)
  end

  # Public API to start a new node process under this supervisor
  def start_node(initial_state) do
    spec = {GtrFabric.Node, initial_state}
    DynamicSupervisor.start_child(__MODULE__, spec)
  end
  
  # Stop a node process
  def stop_node(node_id) do
    case Registry.lookup(GtrFabric.NodeRegistry, node_id) do
      [{pid, _}] -> DynamicSupervisor.terminate_child(__MODULE__, pid)
      [] -> {:error, :not_found}
    end
  end
end
