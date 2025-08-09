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

use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmotionalState {
    pub valence: f32,
    pub arousal: f32,
    pub dominance: f32,
    pub confidence: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionOutcome {
    pub success: bool,
    pub quality_score: f32,
    pub feedback: String,
    pub metadata: serde_json::Value,
}
impl Default for InteractionOutcome {
    fn default() -> Self {
        Self {
            success: false,
            quality_score: 0.0,
            feedback: String::new(),
            metadata: serde_json::json!({}),
        }
    }
}
