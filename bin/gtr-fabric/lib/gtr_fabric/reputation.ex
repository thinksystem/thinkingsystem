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

defmodule GtrFabric.Reputation do
  @moduledoc """
  This module contains functions for calculating and updating reputation scores.
  It operates on a PerformanceLedger, which is the factual history of interactions.
  The functions here are heuristics for interpreting that history.
  """

  @doc "Updates a trust score after a successful task."
  def update_trust_score_on_success(score, success_weight) do
    case GtrFabric.CoreWrapper.trust_success(score, success_weight) do
      {:ok, v} -> v
      {:error, _} -> score
    end
  end

  @doc "Updates a trust score after a failed task, using the dynamic failure weight."
  def update_trust_score_on_failure(score, slash_percentage, params) do
    case GtrFabric.CoreWrapper.trust_failure(score, slash_percentage, params) do
      {:ok, v} -> v
      {:error, _} -> score
    end
  end

  @doc "Decays a trust score over a period of time based on the dynamic lambda."
  def decay_trust_score_continuously(score, seconds_elapsed, params) do
    case GtrFabric.CoreWrapper.trust_decay(score, seconds_elapsed, params) do
      {:ok, v} -> v
      {:error, _} -> score
    end
  end

  @doc """
  Calculates a new TrustScore from a PerformanceLedger.
  This is a pure function that represents the core reputation heuristic.
  """
  def calculate_score_from_ledger(
        %GtrFabric.PerformanceLedger{} = ledger,
        params,
        initial_score \\ 0.5
      ) do
    # Sort records from oldest to newest to process them chronologically
    sorted_records = Enum.sort_by(ledger.records, & &1.timestamp, {:asc, DateTime})

    # Fold over the records, updating the score iteratively
    final_score_struct =
      Enum.reduce(
        sorted_records,
        %GtrFabric.TrustScore{value: initial_score, last_updated_ts: 0},
        fn record, acc_score ->
          # First, decay the score based on the time elapsed since the last update.
          time_diff_secs = DateTime.to_unix(record.timestamp) - acc_score.last_updated_ts

          decayed_score =
            if time_diff_secs > 0 do
              decay_trust_score_continuously(acc_score, time_diff_secs, params)
            else
              acc_score
            end

          # Then, apply the success or failure update
          new_score_struct =
            case record.outcome do
              :success ->
                # For now, use a fixed success weight. This could be dynamic later.
                update_trust_score_on_success(decayed_score, 0.1)

              :failure ->
                # Assume performance_metric holds the basis for slashing
                slash_basis = record.performance_metric

                slash_percentage =
                  GtrFabric.Protocol.calculate_slash_percentage(1.0, slash_basis, params)

                update_trust_score_on_failure(decayed_score, slash_percentage, params)
            end

          %GtrFabric.TrustScore{
            value: new_score_struct.value,
            last_updated_ts: DateTime.to_unix(record.timestamp)
          }
        end
      )

    # Final decay from the last record to now
    time_since_last_event_secs =
      DateTime.to_unix(DateTime.utc_now()) - final_score_struct.last_updated_ts

    if time_since_last_event_secs > 0 do
      decay_trust_score_continuously(
        final_score_struct,
        time_since_last_event_secs,
        params
      )
    else
      final_score_struct
    end
  end
end
