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

# The Elixir module that bridges to the Rust code (optionally).
# For the lightweight demo (provider/topology HTTP only) we do not need the NIFs.
# Set ENABLE_NIFS=1 in the environment before compilation if you want to build the Rust NIFs.

if System.get_env("ENABLE_NIFS") == "1" do
  defmodule GtrFabric.CoreNifs do
    @moduledoc """
    Real NIF bindings (enabled when ENABLE_NIFS=1 at compile time).
    """
    use Rustler, otp_app: :gtr_fabric, crate: "gtr_core"

    # --- Original GTR NIFs ---
    def calculate_potential_value(_node_metrics, _sla), do: :erlang.nif_error(:nif_not_loaded)
    def calculate_forwarding_decision(_candidate_hops, _multipath_threshold), do: :erlang.nif_error(:nif_not_loaded)
    def analyse_dag(_packet_trails, _sla, _total_packets_sent), do: :erlang.nif_error(:nif_not_loaded)

    # --- Dynamic PoPS NIFs ---
    def adjust_parameters_for_epoch_nif(_current_params, _state), do: :erlang.nif_error(:nif_not_loaded)
    def calculate_slash_percentage_nif(_required, _actual, _params), do: :erlang.nif_error(:nif_not_loaded)
    def update_trust_score_on_success_nif(_score, _weight), do: :erlang.nif_error(:nif_not_loaded)
    def update_trust_score_on_failure_nif(_score, _slash, _params), do: :erlang.nif_error(:nif_not_loaded)
    def decay_trust_score_continuously_nif(_score, _seconds_elapsed, _params), do: :erlang.nif_error(:nif_not_loaded)
    def calculate_supplier_offering_nif(_trust, _params), do: :erlang.nif_error(:nif_not_loaded)
    def calculate_consumer_utility_nif(_offering, _trust, _consumer), do: :erlang.nif_error(:nif_not_loaded)
    def create_trust_score_credential_nif(_subject_did, _trust_score, _performance_summary, _issuer_token), do: :erlang.nif_error(:nif_not_loaded)
    def test_add(_a, _b), do: :erlang.nif_error(:nif_not_loaded)
  end
else
  defmodule GtrFabric.CoreNifs do
    @moduledoc """
    Stub implementation (NIFs disabled). All functions raise :nif_disabled.
    This is sufficient for the HTTP provider/topology demo which does not invoke NIF functions.
    Recompile with ENABLE_NIFS=1 to enable native code.
    """
    @error {:error, :nif_disabled}

    # --- Original GTR NIFs (stubs) ---
    def calculate_potential_value(_node_metrics, _sla), do: @error
    def calculate_forwarding_decision(_candidate_hops, _multipath_threshold), do: @error
    def analyse_dag(_packet_trails, _sla, _total_packets_sent), do: @error

    # --- Dynamic PoPS NIFs (stubs) ---
    def adjust_parameters_for_epoch_nif(_current_params, _state), do: @error
    def calculate_slash_percentage_nif(_required, _actual, _params), do: @error
    def update_trust_score_on_success_nif(_score, _weight), do: @error
    def update_trust_score_on_failure_nif(_score, _slash, _params), do: @error
    def decay_trust_score_continuously_nif(_score, _seconds_elapsed, _params), do: @error
    def calculate_supplier_offering_nif(_trust, _params), do: @error
    def calculate_consumer_utility_nif(_offering, _trust, _consumer), do: @error
    def create_trust_score_credential_nif(_subject_did, _trust_score, _performance_summary, _issuer_token), do: @error
    def test_add(_a, _b), do: @error
  end
end
