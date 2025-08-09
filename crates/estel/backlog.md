# Estel Backlog (Multi‑Modal Data Intelligence Engine)

Scope: prioritised work to stabilise the current visual recommendation module, improve quality/performance, and prepare for broader multi‑modal expansion. British English, concise acceptance criteria.

## P0 — Stabilise current release

- Split chart matcher into submodules
  - Deliverables: `src/chart_matcher/` with `config.rs`, `scoring.rs`, `pipeline.rs`, `explain.rs`, `api.rs`
  - Acceptance: crate builds; public `find_qualified_charts*` unchanged; ≥10% file size reduction of any single module
- End‑to‑end suggestion tests
  - Deliverables: fixtures: small CSVs + `config/plotly_api.yml`; tests covering CSV→profiles→matcher→RenderSpec
  - Acceptance: CI green; deterministic golden assertions on top‑N suggestions
- YAML schema validation
  - Deliverables: JSON Schema/YAML schema + validator; CI step to validate `plotly_api.yml`
  - Acceptance: invalid definitions fail CI with clear errors
- Feature‑gate experimental modules
  - Deliverables: cargo features: `intent`, `symbolic`, `data-handler` (default: off)
  - Acceptance: `default` build has no references to experimental paths; `--features intent` builds
- README + docs alignment
  - Deliverables: ensure README snippets compile (doc tests where practical); sequence diagram retained
  - Acceptance: `cargo test` includes doc tests for public API snippets

## P0 — Learnable scoring head (re‑ranking)

- Lightweight learned head (weighted sum) integrating heuristic component scores, dataset stats, and optional symbolic scores
  - Deliverables: `learned_scorer.rs` module (feature‑gated: `learned-scorer`), `ChartSuggestionSystem` helpers to re‑rank
  - Acceptance: crate builds with `--features learned-scorer`; demo able to consume API in follow‑up task
- Weight configuration and defaults
  - Deliverables: `default_head()` weights + re‑exported types; README section with example
  - Acceptance: example compiles; unit test verifying stable feature vector ordering

## P1 — Capability & API enhancements

- Domain hints and scoring profiles in façade
  - Deliverables: extend `ChartSuggestionSystem` to accept `DomainHints` and `ScoringProfile`
  - Acceptance: hints route to matcher; covered by tests
- Explanations API
  - Deliverables: public function to retrieve `ChartExplanation` per suggestion
  - Acceptance: unit test verifies presence of missing requirements / mapping rationale
- Renderer adapter interfaces
  - Deliverables: trait for renderer mappers; adapters for Plotly (Rust/Python bridge) and Vega‑Lite (JSON spec)
  - Acceptance: examples build; mappers unit‑tested for required args
- Multi‑library catalogues
  - Deliverables: support loading multiple YAMLs; filter by library in config
  - Acceptance: tests for cross‑library dedup and selection

## P1 — Data/label bootstrapping

- Implicit feedback capture (clicks/top‑N)
  - Deliverables: simple logging hook returning selected chart names + context
  - Acceptance: integration example logs selections; schema documented
- Heuristic label generation
  - Deliverables: script to generate pseudo‑labels from current matcher scores and symbolic notes
  - Acceptance: produces a CSV suitable for offline training

## P1 — Performance & reliability

- Benchmarks and profiling
  - Deliverables: Criterion benches for profiling + matcher stages; flamegraph instructions
  - Acceptance: baseline numbers recorded in docs; budget set for typical datasets
- Timeouts and backtracking caps
  - Deliverables: config for max candidates, stage timeouts; safe defaults
  - Acceptance: long‑running inputs terminate predictably; tests cover limits
- Result caching
  - Deliverables: optional cache keyed by profiles+config hash
  - Acceptance: cache hit reduces wall time; correctness preserved

## P2 — Data profiling improvements

- Temporal frequency inference & hints
  - Deliverables: infer daily/weekly/monthly; expose in `TemporalStats`
  - Acceptance: tests on synthetic series; affects semantic scoring
- Anomaly/outlier and distribution hints
  - Deliverables: robust z‑score/IQR flags; bimodality/quantisation detection
  - Acceptance: flags appear in `issues`; used as minor penalties in scoring
- Missing data strategies
  - Deliverables: null handling summary + simple imputation suggestions in summary
  - Acceptance: README section + unit tests

## P2 — Learning improvements

- Tiny MLP inference path (optional)
  - Deliverables: an extra feature flag for a 1–2 layer MLP with hardcoded weights
  - Acceptance: parity with weighted sum on test vectors; doc on exporting weights
- Information‑theoretic features
  - Deliverables: add entropy/correlation proxies into feature vector (gated)
  - Acceptance: unit tests of feature computation on small synthetic data

## P3 — Research & expansion

- Optional symbolic integration
  - Deliverables: feature‑flagged blend of neuro‑symbolic score into final ranking
  - Acceptance: integration test toggling weight produces monotonic changes
- Intent workflow stabilisation
  - Deliverables: fix module paths, minimal stable API for ingest→intent→suggest
  - Acceptance: gated by `intent` feature; basic scenario test
- Visual extensions: 3D & temporal
  - Deliverables: expand YAML with 3D/temporal charts; mapping helpers
  - Acceptance: new suggestions appear when suitable data present

## P3 — Symbolic + learned blend research

- Sensitivity analysis of symbolic weight
  - Deliverables: experiment doc; script to sweep symbolic contribution and plot NDCG@k
  - Acceptance: documented findings; recommended default range

## Tooling / CI / Quality

- CI: clippy, rustfmt, cargo‑deny, typos
  - Deliverables: workflows; zero warnings on stable toolchain
  - Acceptance: PRs blocked on failures
- SemVer & features
  - Deliverables: documented feature flags; CHANGELOG
  - Acceptance: release guide; version bump policy

## Tooling / CI for learning

- Unit tests
  - Deliverables: tests for `FeatureVector` ordering and `predict()` clamping
  - Acceptance: `cargo test` green with feature enabled

## Documentation

- API reference and module docs
  - Deliverables: rustdoc comments; `cargo doc` clean
  - Acceptance: key types and functions documented with examples
- Contribution guide
  - Deliverables: `CONTRIBUTING.md`, coding standards, test/bench instructions
  - Acceptance: referenced from README

## Security & Licensing

- Licence headers & audit
  - Deliverables: header script coverage; third‑party notices for YAML/renderer deps
  - Acceptance: audit checklist in repo
