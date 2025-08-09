# Estel Chart Demo

A minimal GUI that profiles a CSV, suggests compatible Plotly charts, and renders them via a small Python helper. It also previews a symbolic (neuro‑symbolic rules) evaluation for each suggestion.

The `estel` crate provides:

- Data profiling (numeric/categorical/temporal detection, quality stats)
- API graph of Plotly chart capabilities
- Chart matching to produce a RenderSpec (chart + mappings)
- Optional symbolic filtering (feature‑gated) for heuristics feedback

This demo wires those pieces into an eframe/egui desktop app.

## Prerequisites

- Rust toolchain (stable)
- Python 3 available as `python3`
- Plotly python helper present at one of these paths (the app adds them to `sys.path`):
  - `python_helpers/`
  - `bin/demos/estel-chart-demo/python_helpers/`
  - `crates/estel/python_helpers/`
  - `../../../crates/estel/python_helpers/`

A working helper is included in `crates/estel/python_helpers/renderer.py`.

## How to Run

From the workspace root:

```sh
cargo run --bin estel-chart-demo
```

Or from this demo directory:

```sh
cargo run
```

Then select a CSV file in the UI.

## Features

- Profiles columns and computes dataset summary
- Finds compatible Plotly charts based on types and quality
- Renders a preview (raw JSON and optional temp HTML/open in browser)
- Symbolic Evaluation (preview): per‑suggestion score + notes based on rules and inferred relationships

## Architecture Diagram

<details>
<summary>Click to expand the sequence diagram</summary>

::: mermaid

---

config:
theme: neutral

---

sequenceDiagram
participant User
participant App as EGUI App
participant Prof as DataProfiler
participant Match as ChartMatcher
participant API as ApiGraph
participant Sym as GraphAwareSymbolicEngine
participant Py as Python Renderer
participant Browser
Note over User,Browser: Estel Chart Demo Flow

rect rgb(240, 248, 255)
Note over App,API: System Initialisation
App->>API: load plotly_api.yml (multiple candidate paths)
API-->>App: api_graph ready
end

User->>App: Select CSV
App->>Prof: profile_csv(path)
Prof-->>App: Vec<DimensionProfile>, DatasetSummary

App->>Match: find_qualified_charts(profiles, API, config)
Match->>API: query chart specs
API-->>Match: compatible charts
Match-->>App: Vec<RenderSpec>

loop For each suggestion
App->>Sym: enhanced_evaluate(ChartSpec, goal)
Sym-->>App: (score, notes)
end

alt Render preview
App->>Py: render_chart(chart, data_json, mappings)
Note over App,Py: App injects python_helpers search paths into sys.path
Py-->>App: plotly JSON
App-->>User: Display JSON preview
else Generate HTML
App->>Py: create_temp_html_chart(...)
Py-->>App: html_path
App->>Browser: open file://html_path
Browser-->>User: Interactive chart
end

:::

</details>

## Symbolic Evaluation Notes

The demo builds a minimal ChartSpec from the suggestion mappings and the profiler’s column types:

- Maps estel DataType: Numeric/Categorical/Temporal
- Infers an AnalysisGoal from chart type and whether a temporal field is involved
- Runs GraphAwareSymbolicEngine::enhanced_evaluate, which augments a baseline rules engine with simple relationship hints

Notes are shown under each suggestion in a collapsing “Symbolic Evaluation” section.

## Troubleshooting

- If rendering fails with `ModuleNotFoundError: renderer`, ensure the python_helpers path contains `renderer.py` and is one of the listed locations.
- If no charts are suggested, try lowering the quality threshold in the left "Configuration" panel.
- Large CSVs are sampled to 100 rows for preview rendering.

## License

AGPL-3.0-only. Copyright (C) 2024 Jonathan Lee.
