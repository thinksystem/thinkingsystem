# QUIC Relay Demo

This demo spins up three local iroh nodes (Alice, Bob, Charlie) and exchanges simple JSON messages between them over QUIC. Each node uses the unified local-first LLM adapter (from STELE) to generate a reply; if a local model isn’t available, it falls back to a simple stub.

Run three terminals:

Terminal 1 (relay):

- Starts a relay endpoint.

Terminal 2 (alice):

- Starts an endpoint with a static secret key, connects to Bob, sends a greeting, and prints the response.

Terminal 3 (bob):

- Starts endpoints with static secret keys, listens for connections, and replies using the local LLM adapter when available.

Quick start (triad chatter mode)

- Build:
  - cargo build -p quic-relay-demo
- Run three nodes in one process with endless chatter:
  - target/debug/quic-relay-demo --triad <HEX_A>,<HEX_B>,<HEX_C>
    - Use three 32-byte hex secrets (e.g., from `openssl rand -hex 32`).
    - The program prints each node’s node_id, relay URL, and direct addresses and then starts a round-robin message loop.
    - Stop with Ctrl+C.

Notes on local LLM

- The demo uses `stele::llm::unified_adapter::UnifiedLLMAdapter::with_defaults()` which prefers a local provider (e.g., Ollama) when detected and can fall back to remote providers if configured via environment variables. If no providers are available, the demo uses a deterministic stub so messages still flow.

You can also run `charlie` similarly and have it send/receive with the others.

Note: This demo uses iroh's Endpoint API and requires network permissions. It binds to localhost UDP.

target/debug/quic-relay-demo --triad $(openssl rand -hex 32),$(openssl rand -hex 32),$(openssl rand -hex 32)
