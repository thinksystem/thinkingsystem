# Contributing (Temporarily Limited)

> Status: The project is still consolidating core architectural invariants across primary crates (STEEL, STELE, SLEET, SCRIBES/ESTEL). External feature / code contributions are **not yet open**.
>
> A highly automated contribution & review pipeline (covering lint, security, architectural conformance, gap tracking) will be introduced shortly. Until that is live:
>
> Allowed now (lightweight only):
>
> - Filing focused issues: bug reports, clarifying questions, documentation errata.
> - Very small documentation corrections (typos / factual fixes) — subject to maintainer discretion.
>
> Not accepted yet:
>
> - New features, refactors, API changes, performance rewrites.
> - Large documentation restructures.
>
> Rationale:
>
> - Prevent churn while stabilising internal module boundaries.
> - Prepare automated gates (style, security, determinism, reproducibility) before accepting code.
>
> Next Steps (Maintainer Roadmap):
>
> - Publish automated contribution pipeline & bot-assisted PR template.
> - Open clearly labelled "good first issue" set once invariants freeze.
> - Provide machine-verifiable specification fragments for critical subsystems.
>
> If you have a strategic proposal, open an issue prefixed `proposal:` with a concise abstract; detailed design docs are premature before the automation lands.
>
> Thank you for your interest. Please watch the repository for the announcement enabling structured contributions.

## Maintainer notes: Automated PR pipeline

This repository uses GitHub Actions to gate merges into `master`:

- merge-guard (on pull_request):

  - cargo fmt, clippy (deny warnings), check, and test across the workspace
  - cargo-deny if `deny.toml` is present
  - typos (British spelling per `typos.toml`)
  - custom check to prevent new `//` code comments in changed Rust lines
  - uploads a `diff.patch` artifact for each PR

- auto-merge (on pull_request_target with label `auto-merge`):
  - Squash merges once all checks succeed
  - Generates a detailed commit message including file-level stats and PR body

Branch protection should require the merge-guard job to succeed. To trigger an automated squash merge, apply the `auto-merge` label to the PR once it's ready.

## Local development automation (git hooks)

To mirror CI checks locally and automate routine cleanup, this repo provides pre-commit and pre-push hooks. They auto-fix spelling and formatting, block forbidden files, enforce the no-inline-comments policy for Rust, and gate pushes with quick CI-like checks.

### Install once

```bash
bash scripts/hooks/install-local-hooks.sh
```

Recommended tools:

- typos-cli (spelling per `typos.toml`):
  - Homebrew: `brew install typos-cli`
  - or Cargo: `cargo install typos-cli`
- Rust toolchain (includes rustfmt and clippy): https://rustup.rs

### What runs on commit

1. Forbidden paths are blocked (e.g., `.env`, `models/`, `tmp/`, local DB dumps).

2. Spelling auto-fix on staged files using `typos.toml`:

- `typos -w` runs on staged files only, then the hook re-stages them.
- A verification pass (`typos --format brief`) ensures no remaining issues.

3. Rust formatting on staged `.rs` files:

- `rustfmt` formats the files and the hook re-stages them.

4. No inline code comments in newly added Rust lines:

- `scripts/ci/check_no_comments.sh staged` fails the commit if `//` comments are added in changed lines.

Bypass in exceptional cases: `git commit --no-verify`.

### What runs on push

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features -- --nocapture`
- `typos --format brief` (if `typos` is installed)

Push is aborted if any step fails. Bypass in exceptional cases: `git push --no-verify`.

### Run checks manually

```bash
# Spelling (auto-fix / verify)
typos -w
typos --format brief

# Comment policy (all files / staged / changed vs base)
bash scripts/ci/check_no_comments.sh all
bash scripts/ci/check_no_comments.sh staged
GITHUB_BASE_REF=master bash scripts/ci/check_no_comments.sh changed

# CI parity checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features -- --nocapture
```

Notes:

- The “changed” mode in the comment checker uses `GITHUB_BASE_REF`; set it to `master` locally for consistent results with CI.
- The hooks operate only on staged files where possible to keep commits fast and focused.
