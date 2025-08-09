# SPDX-License-Identifier: AGPL-3.0-only

ExUnit.start()

defmodule RefreshFlowTest do
  use ExUnit.Case, async: false

  test "refresh lifecycle transitions" do
    # Ensure starting state
    %{phase: :ready} = GtrFabric.RefreshManager.status()

    assert {:ok, :pre_drain} == GtrFabric.RefreshManager.begin_refresh(3)
    %{phase: :pre_drain} = GtrFabric.RefreshManager.status()

    # Wait past pre-drain + rebuild
    Process.sleep(260)
    # Should have completed and be ready again
    %{phase: :ready} = GtrFabric.RefreshManager.status()
  end

  test "conflicting refresh rejected" do
    {:ok, :pre_drain} = GtrFabric.RefreshManager.begin_refresh(2)
    {:error, {:invalid_state, :pre_drain}} = GtrFabric.RefreshManager.begin_refresh(4)
    Process.sleep(260)
  end
end
