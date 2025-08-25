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


use std::hash::{Hash, Hasher};
use tracing::{info, warn};


pub fn persist_plan_artifact(dir: &str, plan: &serde_json::Value, directive: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    directive.hash(&mut hasher);
    let hash_hex = format!("{:08x}", (hasher.finish() as u32));
    
    let mut tokens: Vec<String> = directive
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect();
    tokens.dedup();
    if tokens.len() > 5 { tokens.truncate(5); }
    for t in tokens.iter_mut() { if t.len() > 10 { t.truncate(10); } }
    let slug = if tokens.is_empty() { "untitled".to_string() } else { tokens.join("-") };
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let mut filename = format!("plan_{ts}_{hash_hex}_{slug}.json");
    const MAX_FILENAME: usize = 120; 
    if filename.len() > MAX_FILENAME {
        
        let over = filename.len() - MAX_FILENAME;
        if over > 0 {
            let keep = slug.len().saturating_sub(over + 3);
            let new_slug = if keep > 0 { slug[..keep].to_string() } else { "dir".into() };
            filename = format!("plan_{ts}_{hash_hex}_{new_slug}.json");
        }
    }
    let path = format!("{dir}/{filename}");
    let json = serde_json::to_string_pretty(plan)?;
    let mut final_path = path.clone();
    if final_path.len() > 240 { 
        final_path = format!("{dir}/plan_{hash_hex}.json");
    }
    match std::fs::write(&final_path, &json) {
        Ok(_) => info!(path=%final_path, original_path=%path, "Persisted generated plan artifact"),
        Err(e) => {
            if e.raw_os_error() == Some(63) { 
                let fallback = format!("{dir}/plan_{hash_hex}.json");
                warn!(error=%e, attempted=%final_path, fallback=%fallback, "Filename too long; retrying with minimal fallback name");
                std::fs::write(&fallback, &json)?;
                info!(path=%fallback, "Persisted plan artifact via fallback");
            } else { return Err(anyhow::anyhow!(e)); }
        }
    }
    Ok(())
}
