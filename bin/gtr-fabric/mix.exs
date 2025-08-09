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

# GTR Fabric - Mix Project Configuration
defmodule GtrFabric.MixProject do
  use Mix.Project

  def project do
    enable_nifs? = System.get_env("ENABLE_NIFS") == "1"
    base = [
      app: :gtr_fabric,
      version: "0.1.0",
      elixir: "~> 1.15",
      start_permanent: Mix.env() == :prod,
      deps: deps()
    ]

    if enable_nifs? do
      base ++ [
        rustler: [
          crates: [
            gtr_core: [
              path: "native/gtr_core",
              mode: if(Mix.env() == :prod, do: :release, else: :debug),
              features: ["rustler"]
            ]
          ]
        ]
      ]
    else
      base
    end
  end

  def application do
    [
      extra_applications: [:logger, :crypto],
      mod: {GtrFabric.Application, []}
    ]
  end

  defp deps do
    enable_nifs? = System.get_env("ENABLE_NIFS") == "1"
    rustler_dep = if enable_nifs?, do: [{:rustler, "~> 0.36"}], else: []

    rustler_dep ++ [
      {:joken, "~> 2.5", only: :test},
      {:plug_cowboy, "~> 2.7"},
      {:jason, "~> 1.4"}
    ]
  end
end
