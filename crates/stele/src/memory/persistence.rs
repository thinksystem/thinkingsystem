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

use super::enhanced_memory::RichContext;
use super::memory_components::{Experience, PatternKnowledge};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
#[derive(Debug, Serialize, Deserialize)]
pub struct MemorySnapshot {
    pub version: u32,
    pub experiences: Vec<(usize, Experience)>,
    pub patterns: Vec<(Vec<String>, PatternKnowledge)>,
    pub rich_contexts: Vec<(usize, RichContext)>,
    pub metadata: SnapshotMetadata,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub total_experiences: usize,
    pub total_patterns: usize,
    pub memory_config: MemoryConfigSnapshot,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryConfigSnapshot {
    pub short_term_capacity: usize,
    pub episodic_capacity: usize,
    pub priority_sample_ratio: f32,
    pub temporal_sample_ratio: f32,
    pub pattern_sample_ratio: f32,
}
pub struct MemoryPersistence;
impl MemoryPersistence {
    const CURRENT_VERSION: u32 = 1;
    pub fn save_to_file<P: AsRef<Path>>(
        path: P,
        experiences: &[(usize, Experience)],
        patterns: &[(Vec<String>, PatternKnowledge)],
        rich_contexts: &[(usize, RichContext)],
        config: &super::memory_components::MemoryConfig,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let snapshot = MemorySnapshot {
            version: Self::CURRENT_VERSION,
            experiences: experiences.to_vec(),
            patterns: patterns.to_vec(),
            rich_contexts: rich_contexts.to_vec(),
            metadata: SnapshotMetadata {
                created_at: chrono::Utc::now(),
                total_experiences: experiences.len(),
                total_patterns: patterns.len(),
                memory_config: MemoryConfigSnapshot {
                    short_term_capacity: config.short_term_capacity,
                    episodic_capacity: config.episodic_capacity,
                    priority_sample_ratio: config.priority_sample_ratio,
                    temporal_sample_ratio: config.temporal_sample_ratio,
                    pattern_sample_ratio: config.pattern_sample_ratio,
                },
            },
        };
        let json = serde_json::to_string_pretty(&snapshot)?;
        fs::write(path, json)?;
        Ok(())
    }
    pub fn load_from_file<P: AsRef<Path>>(
        path: P,
    ) -> Result<MemorySnapshot, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        let snapshot: MemorySnapshot = serde_json::from_str(&json)?;
        if snapshot.version != Self::CURRENT_VERSION {
            eprintln!(
                "Warning: Loading memory snapshot with version {} (current: {})",
                snapshot.version,
                Self::CURRENT_VERSION
            );
        }
        Ok(snapshot)
    }
    pub fn save_binary<P: AsRef<Path>>(
        path: P,
        experiences: &[(usize, Experience)],
        patterns: &[(Vec<String>, PatternKnowledge)],
        rich_contexts: &[(usize, RichContext)],
        config: &super::memory_components::MemoryConfig,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        let snapshot = MemorySnapshot {
            version: Self::CURRENT_VERSION,
            experiences: experiences.to_vec(),
            patterns: patterns.to_vec(),
            rich_contexts: rich_contexts.to_vec(),
            metadata: SnapshotMetadata {
                created_at: chrono::Utc::now(),
                total_experiences: experiences.len(),
                total_patterns: patterns.len(),
                memory_config: MemoryConfigSnapshot {
                    short_term_capacity: config.short_term_capacity,
                    episodic_capacity: config.episodic_capacity,
                    priority_sample_ratio: config.priority_sample_ratio,
                    temporal_sample_ratio: config.temporal_sample_ratio,
                    pattern_sample_ratio: config.pattern_sample_ratio,
                },
            },
        };
        let file = fs::File::create(path)?;
        let mut encoder = GzEncoder::new(file, Compression::default());
        let json = serde_json::to_string(&snapshot)?;
        encoder.write_all(json.as_bytes())?;
        encoder.finish()?;
        Ok(())
    }
    pub fn load_binary<P: AsRef<Path>>(
        path: P,
    ) -> Result<MemorySnapshot, Box<dyn std::error::Error>> {
        use flate2::read::GzDecoder;
        use std::io::Read;
        let file = fs::File::open(path)?;
        let mut decoder = GzDecoder::new(file);
        let mut json = String::new();
        decoder.read_to_string(&mut json)?;
        let snapshot: MemorySnapshot = serde_json::from_str(&json)?;
        if snapshot.version != Self::CURRENT_VERSION {
            eprintln!(
                "Warning: Loading memory snapshot with version {} (current: {})",
                snapshot.version,
                Self::CURRENT_VERSION
            );
        }
        Ok(snapshot)
    }
}
