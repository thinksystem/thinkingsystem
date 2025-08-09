# SPDX-License-Identifier: AGPL-3.0-only

defmodule GtrFabric.CoreWrapper do
  @moduledoc """
  Unified safe interface over NIF functions.

  Guarantees:
  - Always returns {:ok, value} or {:error, reason}
  - Maps {:error, reason} from NIF to {:error, {:nif, reason}} (except :nif_disabled passthrough)
  - Catches exceptions / exits and normalises.
  """

  alias GtrFabric.CoreNifs, as: N

  # Public API
  def consumer_utility(offering, trust, consumer), do: wrap(:calculate_consumer_utility_nif, [offering, trust, consumer])
  def supplier_offering(trust, params), do: wrap(:calculate_supplier_offering_nif, [trust, params])
  def adjust_parameters(params, state), do: wrap(:adjust_parameters_for_epoch_nif, [params, state])
  def slash_percentage(req, actual, params), do: wrap(:calculate_slash_percentage_nif, [req, actual, params])
  def trust_success(score, weight), do: wrap(:update_trust_score_on_success_nif, [score, weight])
  def trust_failure(score, slash, params), do: wrap(:update_trust_score_on_failure_nif, [score, slash, params])
  def trust_decay(score, secs, params), do: wrap(:decay_trust_score_continuously_nif, [score, secs, params])
  def potential_value(metrics, sla), do: wrap(:calculate_potential_value, [metrics, sla])
  def forwarding_decision(hops, threshold), do: wrap(:calculate_forwarding_decision, [hops, threshold])
  def analyse_dag(trails, sla, total), do: wrap(:analyse_dag, [trails, sla, total])
  def create_trust_vc(subject_did, trust_score, perf_summary, issuer_token), do: wrap(:create_trust_score_credential_nif, [subject_did, trust_score, perf_summary, issuer_token])

  # Generic wrapper
  defp wrap(fun, args) do
    try do
      case apply(N, fun, args) do
        {:error, :nif_disabled} = e -> e
        {:ok, v} -> {:ok, v}
        {:error, reason} -> {:error, {:nif, reason}}
        other -> {:ok, other}
      end
    rescue
      e -> {:error, {:exception, e.__struct__}}
    catch
      :exit, reason -> {:error, {:exit, reason}}
    end
  end
end
