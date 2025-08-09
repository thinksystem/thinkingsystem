# SPDX-License-Identifier: AGPL-3.0-only

defmodule GtrFabric.RefreshManager do
  @moduledoc """
  Orchestrates graceful refresh cycles.
  States: :ready | :pre_drain | :rebuilding
  """
  use GenServer
  require Logger

  @type state :: %{phase: :ready | :pre_drain | :rebuilding, target: non_neg_integer() | nil}
  @pre_drain_ms 200

  # Public API
  def start_link(opts), do: GenServer.start_link(__MODULE__, %{}, opts)
  def status, do: GenServer.call(__MODULE__, :status)
  def begin_refresh(count), do: GenServer.call(__MODULE__, {:begin, count}, 5_000)

  @impl true
  def init(_), do: {:ok, %{phase: :ready, target: nil}}

  @impl true
  def handle_call(:status, _from, s), do: {:reply, s, s}

  def handle_call({:begin, count}, _from, %{phase: :ready} = s) when is_integer(count) and count > 0 do
    Logger.info("Refresh: pre_drain target=#{count}")
    Process.send_after(self(), {:advance, :rebuild}, @pre_drain_ms)
    {:reply, {:ok, :pre_drain}, %{s | phase: :pre_drain, target: count}}
  end
  def handle_call({:begin, _}, _from, s), do: {:reply, {:error, {:invalid_state, s.phase}}, s}

  @impl true
  def handle_info({:advance, :rebuild}, %{phase: :pre_drain, target: t} = s) do
    Logger.info("Refresh: rebuilding target=#{t}")
    ids = GtrFabric.refresh_demo_nodes(t)
    Logger.info("Refresh: new node set size=#{length(ids)}")
    Process.send_after(self(), {:advance, :ready}, 10)
    {:noreply, %{s | phase: :rebuilding}}
  end
  def handle_info({:advance, :ready}, %{phase: :rebuilding} = s) do
    Logger.info("Refresh: ready")
    {:noreply, %{s | phase: :ready, target: nil}}
  end
  def handle_info(_, s), do: {:noreply, s}
end
