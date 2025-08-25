// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2024 Jonathan Lee
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License version 3
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see https://www.gnu.org/licenses/.


use serde::Serialize;
use std::hash::{Hash, Hasher};
use tracing::{info, warn};

#[derive(Serialize)]
pub struct WatMetadata<'a> {
    pub function: &'a str,
    pub suffix: &'a str,
    pub passes_applied: Vec<&'a str>,
    pub loops: u32,
    pub arithmetic_ops: u32,
    pub param_reads: u32,
    pub length_bytes: usize,
    pub rng_canonicalized: bool,
}

pub fn persist_wat_artifact(
    dir: &str,
    function_name: &str,
    wat: &str,
    suffix: &str,
) -> anyhow::Result<String> {
    std::fs::create_dir_all(dir)?;
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    wat.hash(&mut hasher);
    let h = format!("{:x}", hasher.finish());

    let mut fn_slug: String = function_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if fn_slug.is_empty() {
        fn_slug = "fn".into();
    }
    if fn_slug.len() > 24 {
        fn_slug.truncate(24);
    }
    let mut base = format!(
        "wat_{ts}_{}_{}_{}.wat",
        &h[..8.min(h.len())],
        fn_slug,
        suffix
    );
    const MAX_FILENAME: usize = 100;
    if base.len() > MAX_FILENAME {
        let overflow = base.len() - MAX_FILENAME;
        if overflow > 0 && fn_slug.len() > overflow {
            fn_slug.truncate(fn_slug.len() - overflow);
        }
        base = format!(
            "wat_{ts}_{}_{}_{}.wat",
            &h[..8.min(h.len())],
            fn_slug,
            suffix
        );
    }
    let mut full = format!("{dir}/{base}");
    if full.len() > 240 {
        full = format!("{dir}/wat_{ts}_{}.wat", &h[..8.min(h.len())]);
    }
    match std::fs::write(&full, wat) {
        Ok(_) => {
            info!(function=%function_name, path=%full, suffix=%suffix, "Persisted WAT artifact (lib)");
            Ok(full)
        }
        Err(e) => {
            if e.raw_os_error() == Some(63) {
                let fallback = format!("{dir}/wat_{ts}_{}.wat", &h[..8.min(h.len())]);
                std::fs::write(&fallback, wat)?;
                warn!(function=%function_name, error=%e, path=%fallback, "Filename too long; used fallback for WAT artifact");
                Ok(fallback)
            } else {
                Err(anyhow::anyhow!(e))
            }
        }
    }
}

pub fn persist_wat_sidecar(path: &str, meta: &WatMetadata) -> anyhow::Result<()> {
    let json_path = format!("{path}.json");
    std::fs::write(&json_path, serde_json::to_vec_pretty(meta)?)?;
    info!(path=%json_path, "Persisted WAT metadata sidecar (lib)");
    Ok(())
}

#[derive(Serialize)]
pub struct RustFnMetadata<'a> {
    pub directive: &'a str,
    pub args: &'a [f64],
    pub magnitude: Option<(f64, f64)>,
    pub source_len: usize,
    pub hash: String,
}

pub fn persist_rust_fn_artifacts(
    dir: &str,
    directive: &str,
    source: &str,
    args: &[f64],
    magnitude: Option<(f64, f64)>,
    raw_json: &serde_json::Value,
) -> anyhow::Result<(String, String, String)> {
    std::fs::create_dir_all(dir)?;
    // Short hashed directive slug (stable for identical directive text)
    let mut hasher_dir = std::collections::hash_map::DefaultHasher::new();
    directive.hash(&mut hasher_dir);
    let dir_hash = format!("{:08x}", (hasher_dir.finish() as u32));
    // Sample first 3 alnum tokens (<=8 chars each)
    let mut tokens: Vec<String> = directive
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect();
    if tokens.len() > 3 {
        tokens.truncate(3);
    }
    for t in tokens.iter_mut() {
        if t.len() > 8 {
            t.truncate(8);
        }
    }
    let safe_tokens = if tokens.is_empty() {
        "d".into()
    } else {
        tokens.join("-")
    };
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    let h = format!("{:x}", hasher.finish());
    let mut code_name = format!("rustfn_{ts}_{dir_hash}_{safe_tokens}_{h:.8}.rs");
    if code_name.len() > 110 {
        // shrink token slug
        code_name = format!("rustfn_{ts}_{dir_hash}_{h:.8}.rs");
    }
    let code_path = format!("{dir}/{code_name}");
    std::fs::write(&code_path, source)?;
    let meta = RustFnMetadata {
        directive,
        args,
        magnitude,
        source_len: source.len(),
        hash: h.clone(),
    };
    let meta_path = format!("{code_path}.json");
    std::fs::write(&meta_path, serde_json::to_vec_pretty(&meta)?)?;
    let mut raw_name = format!("rustfn_{ts}_{dir_hash}_{safe_tokens}_{h:.8}_raw.json");
    if raw_name.len() > 115 {
        raw_name = format!("rustfn_{ts}_{dir_hash}_{h:.8}_raw.json");
    }
    let raw_path = format!("{dir}/{raw_name}");
    std::fs::write(&raw_path, serde_json::to_vec_pretty(raw_json)?)?;
    info!(code=%code_path, meta=%meta_path, raw=%raw_path, "Persisted Rust fn artifacts (lib)");
    Ok((code_path, meta_path, raw_path))
}
