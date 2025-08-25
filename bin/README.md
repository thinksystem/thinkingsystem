# thinking-system

Unified control-plane binary (HTTP + QUIC) for local-first LLM ops, agent orchestration, data / knowledge ingestion, and policy‑guarded exchanges.

Primary capability groups:

- QUIC endpoint (iroh) for peer data exchange (ALPN `steel/data-exchange/0`)
- Data Exchange providers (dynamic + config driven) & health
- LLM completion + streaming over a unified adapter
- Agent registry (register / list / invoke / status update + persistence)
- Policy engine & Verifiable Credential issuance
- Optional NLU ingestion (dynamic + canonical graph + embeddings) and summary stats
- Lightweight Knowledge Graph (fact ingest + query + optional in-memory enrichment)
- Audit ring buffer & runtime metrics
- SurrealDB integration (dynamic graph + optional canonical / knowledge)

## Environment Variables

Core:

- TS_HTTP_ADDR (default 127.0.0.1:8080)
- TS_BODY_LIMIT_BYTES (max request body, default 65536)
- RUST_LOG (e.g. `info`, `debug`)
- TS_CONFIG_FILE (default `config/system.toml`) for `[data_exchange]` providers
- TS_POLICY_FILE (optional path to policy YAML; enables policy engine)

Auth / Identity:

- TS_JWT_SECRET / TS_JWT_ISSUER / TS_JWT_AUDIENCE
- TS_ISSUER_DID (default `did:steel:local-issuer`)
- TS_ISSUER_SECRET_BASE64 (optional deterministic Ed25519 seed for VC issuance)

Agents:

- TS_AGENT_PERSIST=1 enable persistence
- TS_AGENT_STORE_PATH=path to JSON persistence file

Database (optional SurrealDB dynamic store):

- SURREALDB_URL / SURREALDB_USER / SURREALDB_PASS / SURREALDB_NS / SURREALDB_DB

Canonical (advanced NLU / bitemporal flows if using stele regulariser):

- STELE_CANON_URL / STELE_CANON_USER / STELE_CANON_PASS / STELE_CANON_NS / STELE_CANON_DB

NLU Runtime (optional):

- TS_ENABLE_NLU=1 to initialise NLU components and enable `/nlu/*` endpoints

Knowledge Scribe / KG enrichment (optional):

- TS_ENABLE_KNOWLEDGE=1 enables in‑memory entity linking during ingestion & KG memory summaries

LLM provider keys (optional fallbacks):

- OPENAI_API_KEY, ANTHROPIC_API_KEY, etc.

## Endpoints Summary

| Path                             | Method   | Description                                                                        |
| -------------------------------- | -------- | ---------------------------------------------------------------------------------- |
| /health                          | GET      | Liveness check                                                                     |
| /status                          | GET      | Node (QUIC) identity & addresses                                                   |
| /metrics                         | GET      | JSON metrics (uptime_secs, llm_models, agent_count, counters)                      |
| /db/health                       | GET      | SurrealDB availability (connected bool)                                            |
| /audit                           | GET      | Snapshot of recent audit events (in-memory ring)                                   |
| /analysis/last                   | GET      | Last analysis correlation id (if any)                                              |
| /exchange                        | POST     | Data exchange request (provider inline or pre-configured)                          |
| /exchange/providers              | GET      | List configured providers                                                          |
| /exchange/providers/add          | POST     | Dynamically add a provider                                                         |
| /exchange/providers/:name/remove | POST     | Remove a provider                                                                  |
| /exchange/providers/:name/health | GET      | Provider health info                                                               |
| /llm/models                      | GET      | Available LLM model identifiers                                                    |
| /llm/complete                    | POST     | Synchronous LLM response                                                           |
| /llm/stream                      | POST     | Server-Sent Events (SSE) streaming response (JSON delta events + error events)     |
| /policy/status                   | GET      | Policy engine load status                                                          |
| /policy/reload                   | POST     | Reload policy engine (TS_POLICY_FILE)                                              |
| /iam/vc/issue                    | POST     | Issue a role credential (requires admin/issuer JWT role)                           |
| /agents                          | GET/POST | List or register agents                                                            |
| /agents/:id/status               | POST     | Update agent status                                                                |
| /agents/:id/invoke               | POST     | Invoke agent (LLM backed, with optional KG memory snippets)                        |
| /nlu/ingest                      | POST     | (Optional) Ingest free text via NLU pipeline                                       |
| /nlu/db/summary                  | GET      | (Optional) Dynamic + canonical counts                                              |
| /kg/ingest                       | POST     | (Optional) Ingest simple facts (subject, predicate, object JSON)                   |
| /kg/query                        | POST     | (Optional) Query facts with filters + optional memory summary if knowledge enabled |

## Authentication & Authorisation

Most mutating or sensitive endpoints require a Bearer JWT (TS_JWT_SECRET). Optionally a Verifiable Credential may be supplied for role assertions. If `TS_POLICY_FILE` is set and loads successfully, fine-grained allow/deny decisions are enforced. Endpoints return consistent JSON error envelopes:

```
{ "error": "message", "code": "optional-code" }
```

Streaming (`/llm/stream`) uses SSE events:

- Normal data: `data: {"delta":"...","final":false}`
- Final chunk has `final: true`
- Errors mid-stream: `event: error` with plain text payload
  Initial auth/validation errors return standard HTTP JSON error envelopes.

## Metrics

`/metrics` returns for example:

```
{
  "uptime_secs": 42,
  "llm_models": 1,
  "agent_count": 3,
  "llm_requests_total": 5,
  "llm_stream_requests_total": 2,
  "exchange_requests_total": 4,
  "nlu_ingest_total": 7,
  "kg_ingest_total": 2,
  "kg_query_total": 4
}
```

All counters are reset on process start (in-memory atomics). Prometheus exposition can be added later if needed.

## NLU Ingestion

Enable with `TS_ENABLE_NLU=1` (requires SurrealDB env vars). Canonical DB and knowledge enrichment optional (set canonical env + `TS_ENABLE_KNOWLEDGE=1`). Ingest request:

```
POST /nlu/ingest
{
  "user": "alice",
  "channel": "chat",
  "text": "Alice met Bob yesterday at 3pm.",
  "vc": { ... optional VC ... }
}
```

Response includes structured extraction & utterance identifiers via Stele QueryProcessor; if knowledge is enabled, entity linking occurs asynchronously.

## Local Run

Build:

```
cargo build -p thinking-system
```

Run (minimal):

```
TS_JWT_SECRET=dev TS_ENABLE_NLU=0 cargo run -p thinking-system
```

With NLU + DB (example):

```
export TS_ENABLE_NLU=1
export SURREALDB_URL=ws://127.0.0.1:8000
export SURREALDB_USER=root
export SURREALDB_PASS=pass
export SURREALDB_NS=dynamic
export SURREALDB_DB=demo
export STELE_CANON_URL=ws://127.0.0.1:8000
export STELE_CANON_USER=root
export STELE_CANON_PASS=pass
export STELE_CANON_NS=canonical
export STELE_CANON_DB=demo
cargo run -p thinking-system
```

## Knowledge Graph

Lightweight fact store with optional enrichment.

Ingest request:

```json
{
  "facts": [
    { "subject": "Alice", "predicate": "met", "object": "Bob" },
    { "subject": "Alice", "predicate": "role", "object": "Engineer" }
  ]
}
```

Response summary fields: accepted, persisted, skipped_invalid, skipped_duplicate.

Query request (all fields optional):

```json
{ "subject": "Alice", "limit": 20 }
```

If `TS_ENABLE_KNOWLEDGE=1` a `memory` field may be returned with enrichment summary.

## Audit

`GET /audit` returns recent in-memory audit events (bounded ring buffer used for quick debugging prior to richer provenance recording).

## Quickstart

Build

```zsh
cargo build -p thinking-system
```

Run (fixed port)

```zsh
RUST_LOG=info TS_HTTP_ADDR=127.0.0.1:18080 cargo run -p thinking-system
```

Health check

```zsh
curl -s http://127.0.0.1:18080/health
```

Status (node info)

```zsh
curl -s http://127.0.0.1:18080/status | jq
```

If the configured port is busy, the supervisor logs the fallback ephemeral port.

Secret key (optional deterministic QUIC identity):

```zsh
openssl rand -hex 32 # set TS_IROH_SECRET_HEX
```

## Dynamic Provider Management

Add provider:

```zsh
curl -s -X POST $HOST/exchange/providers/add \
  -H 'content-type: application/json' \
  -d '{"name":"loop","connection_type":"quic","config":{"alpn":"steel/data-exchange/0","node_id":"<NODE>","relay_url":"<RELAY>","addrs":"<ADDR>"}}'
```

Remove provider:

```zsh
curl -s -X POST $HOST/exchange/providers/loop/remove
```

## LLM API

Endpoints:

- POST /llm/complete → single JSON response
- POST /llm/stream → SSE `{delta,final}` events
- GET /llm/models → list model names

Request shape

```json
{
  "prompt": "Explain QUIC vs HTTP/2",
  "system_prompt": "You are a helpful assistant.",
  "generation": {
    "max_tokens": 512,
    "temperature": 0.7,
    "top_p": 1.0,
    "stream": false
  }
}
```

Examples

```zsh
# Complete (JSON)
curl -s -X POST http://127.0.0.1:18080/llm/complete \
  -H 'content-type: application/json' \
  -H 'authorization: Bearer <JWT>' \
  -d '{"prompt":"hello from supervisor"}' | jq

# Stream (SSE)
curl -N -s -X POST http://127.0.0.1:18080/llm/stream \
  -H 'content-type: application/json' \
  -H 'authorization: Bearer <JWT>' \
  -d '{"prompt":"stream a short response","generation":{"stream":true}}'

# Models
curl -s http://127.0.0.1:18080/llm/models | jq
```

## QUIC endpoint notes

- ALPN: `steel/data-exchange/0`
- Relay: `RelayMode::Default` with discovery n0 enabled.
- On startup, logs include:
  - `node_id` — your peer identity
  - `relay` — home relay URL
  - `addrs_str` — direct socket addresses

Example log lines

```
quic endpoint node_id=... relay=https://... addrs_str=192.168.x.x:5xxxx
control plane listening local=127.0.0.1:18080
```

## Exchange API

POST /exchange supports either a preconfigured provider or an ad-hoc provider in the request.

Ad-hoc QUIC loop (example)

- First, get your node info:
  ```zsh
  curl -s http://127.0.0.1:18080/status | jq -r '.node_id, .relay, (.direct_addrs[0])'
  ```
- Then craft a request (replace placeholders with values from /status):
  ```zsh
  curl -s -X POST http://127.0.0.1:18080/exchange \
    -H 'content-type: application/json' \
  -H 'authorization: Bearer <JWT>' \
    -d '{
      "provider": "self",
      "connection_type": "quic",
      "config": {
        "alpn": "steel/data-exchange/0",
        "node_id": "<NODE_ID>",
        "relay_url": "<RELAY_URL>",
        "addrs": "<DIRECT_ADDR>"
      },
  "message": "hello from /exchange",
      "metadata": { "metadata": { "type": { "String": "event" } } }
    }' | jq
  ```
  Note: This performs a QUIC connection using the provided NodeId/NodeAddr. You can loop back to the same node or target another node.

Preconfigured providers (preferred)

- Define providers in `config/system.toml` and then just call `/exchange` with `{ "provider": "name", "message": "...", "metadata": ... }`.
- Example QUIC provider (you must fill in real peer identity; ALPN shown for completeness):

```toml
[data_exchange]
providers = [
  { name = "peer-a", connection_type = "quic", config = { alpn = "steel/data-exchange/0", node_id = "<NODE_ID>", relay_url = "<RELAY_URL>", addrs = "<DIRECT_ADDRS>" } }
]
```

### Auth options

- JWT: Send Authorization header `Bearer <token>` matching `TS_JWT_*` settings.
- Verifiable Credential: Include a `vc` object in body for POST routes. Example (LLM complete):

```json
{
  "prompt": "Explain QUIC",
  "vc": {
    "@context": [
      "https://www.w3.org/2018/credentials/v1",
      "https://steel.identity/credentials/v1"
    ],
    "type": ["VerifiableCredential", "RoleCredential"],
    "issuer": "did:steel:local-issuer",
    "issuanceDate": "2024-01-01T00:00:00Z",
    "credentialSubject": {
      "id": "did:steel:...",
      "name": "Alice",
      "roles": ["verified"]
    },
    "proof": {
      "type": "Ed25519Signature2020",
      "created": "2024-01-01T00:00:00Z",
      "verificationMethod": "did:steel:local-issuer#key-1",
      "proofPurpose": "assertionMethod",
      "proofValue": "<base64-sig>"
    }
  }
}
```

## Agents

Register

```zsh
curl -s -X POST $HOST/agents \
  -H "authorization: Bearer $JWT" \
  -H 'content-type: application/json' \
  -d '{"name":"alpha","role":"worker"}' -i
```

List

```zsh
curl -s -H "authorization: Bearer $JWT" $HOST/agents | jq
```

Update Status

```zsh
curl -s -X POST $HOST/agents/<ID>/status \
  -H "authorization: Bearer $JWT" \
  -H 'content-type: application/json' \
  -d '{"status":"offline","reason":"planned maintenance"}' | jq
```

Invoke (simple LLM-backed prompt)

```zsh
curl -s -X POST $HOST/agents/<ID>/invoke \
  -H "authorization: Bearer $JWT" \
  -H 'content-type: application/json' \
  -d '{"input":{"prompt":"Summarise the system"}}' | jq
```

Persistence

```zsh
TS_AGENT_PERSIST=1 TS_AGENT_STORE_PATH=/tmp/agents.json cargo run -p thinking-system
```

## Tests & Smoke Coverage

Key tests (run with `cargo test -p thinking-system`):

- `agent_status_persists_across_registry_reloads` – verifies Busy status survives a restart when persistence enabled.
- `http_agent_status_endpoint_smoke` – (opt‑in via `TS_SMOKE_HTTP=1`) exercises `/agents/:id/status`.
- `http_basic_agent_and_vc_flow` – registers an agent, lists, updates status, issues a VC, and hits policy reload (negative path).
- `quic_exchange_loopback` – exercises QUIC round‑trip via /exchange with self-referential provider config.
- `llm_stream_collects_events` – validates SSE streaming returns at least one chunk.

Enable the HTTP smoke tests that require model initialisation:

```zsh
TS_SMOKE_HTTP=1 cargo test -p thinking-system --tests
```

## Roadmap (selected)

- Prometheus / OpenTelemetry exporter
- Structured logging correlation ids
- Pagination & filtering for agents
- Latency + token / cost metrics
- Versioned KG schema & validation
- Provider auth strategies & deeper health probes
