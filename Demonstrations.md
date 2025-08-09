# Thinking System Demonstrations

The demos in `bin/demos` are the first layer above the core crates. They exercise vertical slices of the Thinking System—a sovereign, decentralised cognitive architecture—across different contexts. This document summarises each demo, its purpose, and the core capabilities it showcases.

---

## Bytecode VM demo (SLEET)

Runs structured workflows as bytecode inside the SLEET virtual machine. Ephemeral, capability‑scoped agents execute in sandboxed theatres with gas metering and controlled FFI.

- Highlights:
  - Ephemeral agent creation, role assignment, and parallel execution; teardown on completion.
  - Flow transpilation to bytecode contracts; capability checks and gas controls.
  - Asynchronous dispatch with correlation IDs and response channels.
  - Real‑time execution with timeout/fallback for demo continuity.
- Depends on: `sleet` (agent runtime, VM, scheduler, FFI), optional LLM provider(s).

---

## Enhanced VM demo (SLEET)

Exercises the VM instruction set and execution modes, including the JIT’s hot‑path optimisation.

- Highlights:
  - Arithmetic, control‑flow, stack operations, and FFI invocation patterns.
  - Gas‑metered, deterministic execution with permissioned FFI.
  - Profiling and JIT compilation for frequently executed sequences (Cranelift‑backed).
  - Raw bytecode construction to validate stack state and jump targets.
- Depends on: `sleet` (VM, JIT, FFI, execution status), bytecode utilities.

---

## Flows demo (STELE)

Shows adaptive flow orchestration with LLM‑assisted error recovery and API path negotiation.

- Highlights:
  - Unified multi‑provider LLM adapter (local and remote) with fallback.
  - Self‑healing flow regeneration (bounded iterations) using contextual analysis.
  - Enhanced block results for external data and API response analysis.
  - Dynamic prompt generation informed by block registry metadata.
  - API exploration with schema inference and validation; iterative recovery.
- Depends on: `stele` (flow engine, block registry), `steel` (DB ops), HTTP and LLM adapters.

---

## ESTEL chart demo (ESTEL)

Interactive, intelligent data visualisation from arbitrary CSVs using symbolic reasoning and heuristics (no model training required).

- Highlights:
  - Data profiling with Polars: statistics, temporal detection, cardinality, quality scoring.
  - Chart matching across 25+ types via an API graph and semantic argument mapping.
  - Scoring for feasibility, semantics, visual effectiveness, and utilisation.
  - Explanatory rule‑based filtering and intent inference (trend, comparison, etc.).
  - HTML export via Python Plotly; cross‑platform GUI with egui.
- Depends on: `estel` (profiling, matching, filtering), Polars, Plotly (via Python), egui.

> Note: Some ESTEL modules are pre‑release and will be stabilised for the Q4 2025 milestone.

---

## Main database demo (STELE)

Conversational knowledge management with dynamic query generation and provenance‑anchored storage.

- Highlights:
  - Dual modes: statement analysis (extract + store) and natural‑language search.
  - Dynamic Data Access Layer (DDAL): intent analysis + schema discovery → optimised queries.
  - NLU pipeline: entities, temporal markers, numbers, actions, relations, confidence.
  - SurrealDB storage: entities/relations/utterances with full provenance linking to sources.
  - QueryKG: documentation‑derived patterns that guide query interpretation.
- Depends on: `stele` (NLU, dynamic storage, query generation), SurrealDB, LLM planning.

---

## Scribes demo (STEEL/STELE)

Persistent Scribes provide identity, policy, and data‑persistence mediation. They are the governance and memory backbone complementing ephemeral Agents.

- Highlights:
  - IAM provider with RBAC, token issuing/verification, admin bootstrap, trust scoring.
  - Real‑time LLM orchestration with structured analysis prompts and cost tracking.
  - Comprehensive logging (timings, tokens, costs) and visualisation of activity.
  - Scenario framework for multi‑Scribe workflows and performance instrumentation.
  - Tight integration with STELE for DB/NLU and STEEL for IAM/policy.
- Depends on: `steel` (IAM), `stele` (NLU/DB), SurrealDB, egui, and LLM adapters.

---

## Telegram messaging demo (STEEL)

Bi‑directional Telegram client with on‑device “insight” detection via a hybrid analyser: GLiNER ONNX NER + deterministic syntactic heuristics with percentile thresholds.

- Highlights:
  - Local‑only analysis path (no cloud calls); configurable confidence and entity weights.
  - Colour‑coded risk indicators and review flags; history‑aware thresholds.
  - Async workers, thread‑safe message sharing, and robust connection handling.
  - GUI with chat list, message history, and live entity/risk panels.
- Depends on: `steel` (insight module), egui, GLiNER ONNX model (local). See the demo README for model setup and config keys.

---

## GTR Fabric (ECON)

Decentralised economic fabric and trust/routing layer implemented with Elixir/OTP and Rust NIFs.

- Highlights:
  - Geodesic Traversal Routing (GTR) using multi‑dimensional potential fields.
  - Proof‑of‑Performance staking; adaptive parameters for self‑regulation.
  - Reputation via event‑sourced ledgers with time‑decay and VC issuance.
  - High‑performance distributed design: GenServers, registries, SLA monitors.
  - Blockchain‑agnostic core with pluggable network adapters.
- Depends on: Elixir app with Rust NIFs, `steel` for IAM/VC integration.

---

## Notes on scope and contribution

Each demo contains essential logic that will graduate into the core architecture. They are intentionally opinionated to accelerate learning and harden interfaces before promotion into crates.

Language models are used throughout, but interactions are governed by unified policies and credential management. Local inference is preferred where viable, with prioritisation strategies coupled to the ECON fabric. All configuration honours user sovereignty via Scribe‑mediated control.

---

## Technical note: SurrealDB

Many demos run with an in‑memory SurrealDB; more robust scenarios benefit from a standalone instance. After installing SurrealDB:

```
surreal start --log trace --user root --pass root --bind 127.0.0.1:8000 memory
```

Copyright (C) 2024 Jonathan Lee.
