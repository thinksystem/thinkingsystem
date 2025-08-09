# SPDX-License-Identifier: AGPL-3.0-only

# Integration tests for HTTP surface using direct Plug invocation (no external HTTP client).
ExUnit.start()

defmodule IntegrationHttpTest do
  use ExUnit.Case, async: false
  # import Plug.Test is newer pattern but existing use fine; silence deprecation by importing directly
  import Plug.Test

  setup_all do
    {:ok, _} = Application.ensure_all_started(:gtr_fabric)
    wait_health(30)
    :ok
  end

  test "health endpoint" do
    {code, body} = get_json("/health")
    assert code == 200
    assert body["status"] == "ok"
  end

  test "providers + topology + metrics coherence" do
    {200, providers} = get_json("/providers")
    assert is_list(providers) and length(providers) > 0
    ids = Enum.map(providers, & &1["id"])

    {200, %{"topology" => topo}} = get_json("/topology")
    topo_ids = Enum.map(topo, & &1["id"])
    assert Enum.sort(ids) == Enum.sort(topo_ids)

    {200, %{"providers" => metrics}} = get_json("/metrics")
    metric_ids = Enum.map(metrics, & &1["id"])
    assert Enum.sort(ids) == Enum.sort(metric_ids)
  end

  test "graceful refresh changes provider count" do
    target = 3
    {200, initial} = get_json("/providers")
    init_count = length(initial)
    assert {:accepted, _} = post_refresh(target)
    wait_ready()
    {200, updated} = get_json("/providers")
    assert length(updated) == target
    refute init_count == target or init_count == 0
  end

  # Helpers
  defp get_json(path) do
    conn = request(:get, path)
    {conn.status, decode(conn.resp_body)}
  end

  defp post_refresh(count) do
    conn = request(:post, "/refresh", %{count: count})
    case conn.status do
      202 -> {:accepted, decode(conn.resp_body)}
      code -> {code, decode(conn.resp_body)}
    end
  end

  defp request(method, path, body \\ nil) do
    encoded = if body, do: Jason.encode!(body), else: nil
    conn = Plug.Test.conn(method, path, encoded)
    conn = if body, do: Plug.Conn.put_req_header(conn, "content-type", "application/json"), else: conn
    GtrFabric.HttpRouter.call(conn, [])
  end

  defp wait_ready(attempts \\ 60) do
    {code, body} = get_json("/refresh/status")
    phase = if code == 200, do: body["phase"], else: "unknown"
    cond do
      phase == "ready" -> :ok
      attempts <= 0 -> flunk("refresh did not return to ready, last phase=#{phase}")
      true -> Process.sleep(100); wait_ready(attempts - 1)
    end
  end

  defp wait_health(0), do: :ok
  defp wait_health(n) do
    case get_json("/health") do
      {200, %{"status" => "ok"}} -> :ok
      _ -> Process.sleep(50); wait_health(n - 1)
    end
  end

  defp decode(<<>>), do: %{}
  defp decode(body) when is_binary(body) do
    case Jason.decode(body) do
      {:ok, v} -> v
      _ -> %{"_raw" => body}
    end
  end
end
