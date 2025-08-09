# SPDX-License-Identifier: AGPL-3.0-only

defmodule GtrFabric.HttpRouter do
  use Plug.Router
  require Logger

  plug :match
  plug Plug.Parsers, parsers: [:json], json_decoder: Jason
  plug :dispatch

  get "/providers" do
    send_resp(conn, 200, Jason.encode!(GtrFabric.providers_snapshot()))
  end

  get "/topology" do
    topo = GtrFabric.providers_snapshot() |> Enum.map(fn %{id: id} ->
      case GtrFabric.get_node_state(id) do
        %{connections: conns} -> %{id: id, neighbours: Map.keys(conns)}
        _ -> %{id: id, neighbours: []}
      end
    end)
    send_resp(conn, 200, Jason.encode!(%{topology: topo}))
  end

  get "/health" do
    send_resp(conn, 200, ~s({"status":"ok"}))
  end

  get "/metrics" do
    providers = GtrFabric.providers_snapshot()
    seed = Application.get_env(:gtr_fabric, :seed)
    enriched = Enum.map(providers, fn p ->
      base = case GtrFabric.get_node_state(p.id) do
        %{connections: conns, available_throughput: thr} -> Map.merge(p, %{neighbours: Map.keys(conns), throughput: thr})
        _ -> Map.merge(p, %{neighbours: [], throughput: 0})
      end
      Map.put(base, :latency_ma, 100.0) # placeholder until real tracking
    end)
    send_resp(conn, 200, Jason.encode!(%{providers: enriched, seed: seed}))
  end

  post "/refresh" do
    case GtrFabric.RefreshManager.begin_refresh(extract_target(conn)) do
      {:ok, phase} -> send_resp(conn, 202, Jason.encode!(%{status: to_string(phase)}))
      {:error, {:invalid_state, phase}} -> send_resp(conn, 409, Jason.encode!(%{error: "in_progress", phase: to_string(phase)}))
    end
  end

  get "/refresh/status" do
    %{phase: phase} = GtrFabric.RefreshManager.status()
    send_resp(conn, 200, Jason.encode!(%{phase: to_string(phase)}))
  end

  get "/simulate" do
    sample = Enum.take(GtrFabric.providers_snapshot(), 3)
    path = Enum.map(sample, & &1.id)
    send_resp(conn, 200, Jason.encode!(%{sample_path: path, hops: length(path)}))
  end

  defp extract_target(conn) do
    # Prefer already-parsed params (Plug.Parsers runs before dispatch)
    param_val = conn.params["count"] || conn.params["nodes"]
    cond do
      is_integer(param_val) -> param_val
      is_binary(param_val) ->
        case Integer.parse(param_val) do
          {v, _} -> v
          _ -> fallback_body(conn)
        end
      true -> fallback_body(conn)
    end
  end

  defp fallback_body(conn) do
    case Plug.Conn.read_body(conn) do
      {:ok, body, _} when byte_size(body) > 0 ->
        with {:ok, m} <- Jason.decode(body),
             c when is_integer(c) <- m["count"] || m["nodes"] do
          c
        else
          _ -> 2
        end
      _ -> 2
    end
  end

  match _ do
    send_resp(conn, 404, "not found")
  end
end
