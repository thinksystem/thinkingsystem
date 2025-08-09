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

defmodule GtrFabric.Strategy do
  @moduledoc """
  This module contains the strategic and opinionated functions for the GTR Fabric.
  These functions represent one possible way to "play the game" of the GTR economy.
  Agents can override these with their own custom logic.
  """

  @doc "Calculates a supplier's offering (stake, price) based on their trust and dynamic parameters."
  def calculate_supplier_offering(trust_score, params) do
    case GtrFabric.CoreWrapper.supplier_offering(trust_score, params) do
      {:ok, v} -> v
      {:error, _} -> {:error, :nif_disabled}
    end
  end

  @doc "Calculates the utility for a consumer given a supplier's offering."
  def calculate_consumer_utility(offering, trust_score, consumer) do
    case GtrFabric.CoreWrapper.consumer_utility(offering, trust_score, consumer) do
      {:ok, v} -> v
      {:error, _} -> 0.0
    end
  end
end
