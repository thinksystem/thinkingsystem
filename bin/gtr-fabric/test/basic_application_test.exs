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

defmodule BasicApplicationTest do
  use ExUnit.Case, async: false
  doctest GtrFabric

  @moduledoc """
  Basic application-level tests to ensure the GTR Fabric application
  starts correctly and core dependencies are available.
  """

  test "application starts successfully" do
    # Test that the application can start without errors
    assert {:ok, _} = Application.ensure_all_started(:gtr_fabric)
  end

  test "required modules are loaded" do
    # Verify core modules are available
    assert Code.ensure_loaded?(GtrFabric)
    assert Code.ensure_loaded?(GtrFabric.NodeMetrics)
    assert Code.ensure_loaded?(GtrFabric.CandidateHop)
    assert Code.ensure_loaded?(GtrFabric.Breadcrumb)
    assert Code.ensure_loaded?(GtrFabric.SLA)
  end

  test "supervisor tree is running" do
    # Verify the main supervisor is running
    children = Supervisor.which_children(GtrFabric.Supervisor)
    assert is_list(children)
    assert length(children) > 0
  end
end
