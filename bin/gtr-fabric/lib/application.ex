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

# GTR Fabric Application
defmodule GtrFabric.Application do
  use Application
  require Logger
  import Bitwise

  @impl true
  def start(_type, _args) do
    Logger.info("Starting GTR Fabric Application...")
    port = String.to_integer(System.get_env("PORT") || "4000")

    # Deterministic RNG seeding (optional)
    seed_env = System.get_env("FABRIC_SEED")
    seed =
      case seed_env do
        nil -> :erlang.system_time(:millisecond)
        s -> :erlang.phash2(s)
      end
    a = band(seed, 0xFFFF)
    b = band(bsr(seed, 16), 0xFFFF)
    c = band(bsr(seed, 32), 0xFFFF)
    :rand.seed(:exsss, {a,b,c})
    Application.put_env(:gtr_fabric, :seed, seed)

    # The main supervision tree.
    # It starts a Registry for node lookups and a supervisor for the GTR nodes.
    children = [
      {Registry, keys: :unique, name: GtrFabric.NodeRegistry},
      {GtrFabric.NodeSupervisor, name: GtrFabric.NodeSupervisor},
      {GtrFabric.TaskCollector, name: GtrFabric.TaskCollector},
      {GtrFabric.RefreshManager, name: GtrFabric.RefreshManager},
      {Plug.Cowboy, scheme: :http, plug: GtrFabric.HttpRouter, options: [port: port]}
    ]

    opts = [strategy: :one_for_one, name: GtrFabric.Supervisor]
    with {:ok, pid} <- Supervisor.start_link(children, opts) do
      count = String.to_integer(System.get_env("DEMO_NODE_COUNT") || "2")
      Logger.info("Spawning demo nodes count=#{count}")
      GtrFabric.spawn_demo_nodes(count)
      {:ok, pid}
    end
  end
end
