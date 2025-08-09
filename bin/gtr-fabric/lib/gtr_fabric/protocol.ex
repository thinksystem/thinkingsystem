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

defmodule GtrFabric.Protocol do
  @moduledoc """
  This module defines the core, non-negotiable rules of the GTR Fabric protocol.
  These functions are deterministic and form the basis of consensus between agents.
  """

  @doc """
  Takes the current network state and parameters and returns an updated set of parameters.
  This is the core of the dynamic feedback loop.
  """
  def adjust_parameters_for_epoch(current_params, state) do
    # TODO: Add validations for params and state structs
    GtrFabric.CoreNifs.adjust_parameters_for_epoch_nif(current_params, state)
  end

  @doc "Calculates the percentage of collateral to be slashed based on performance."
  def calculate_slash_percentage(required_performance, actual_performance, params) do
    GtrFabric.CoreNifs.calculate_slash_percentage_nif(
      required_performance,
      actual_performance,
      params
    )
  end
end
