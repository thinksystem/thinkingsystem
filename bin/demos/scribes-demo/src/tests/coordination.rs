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

use crate::demo_processor::DemoDataProcessor;
use crate::identity::EnhancedIdentityVerifier;
use serde_json::json;
use std::sync::Arc;
use stele::scribes::specialists::KnowledgeScribe;
use tracing::debug;

pub async fn test_multi_specialist_coordination(
    knowledge_scribe: &mut KnowledgeScribe,
    enhanced_processor: &Arc<DemoDataProcessor>,
    enhanced_verifier: &Arc<EnhancedIdentityVerifier>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    debug!("Testing multi-specialist coordination");
    let mut handoffs_completed = 0;
    let research_data = json!({
        "text": "Emergent Behaviours in Multi-Agent Systems",
        "source_id": "urn:stele:log:1138",
        "urgency": 0.6
    });
    if enhanced_verifier
        .verify_source(&research_data)
        .await
        .is_ok()
    {
        handoffs_completed += 1;
        if let Ok(_data_result) = enhanced_processor.process_data(&research_data).await {
            handoffs_completed += 1;
            let knowledge_context =
                json!({ "entities": ["multi_agent_systems"], "content": research_data["text"] });
            if knowledge_scribe
                .link_data_to_graph(&knowledge_context)
                .await
                .is_ok()
            {
                handoffs_completed += 1;
            }
        }
    }
    Ok(json!({ "handoffs_completed": handoffs_completed }))
}
