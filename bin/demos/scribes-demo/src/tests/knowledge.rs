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

use crate::data_loader::DataLoader;
use serde_json::json;
use stele::scribes::specialists::KnowledgeScribe;
use tracing::debug;

pub async fn test_knowledge_specialist(
    knowledge_scribe: &mut KnowledgeScribe,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    debug!("Testing KnowledgeScribe API");
    let test_scenarios = DataLoader::load_knowledge_specialist_scenarios()?;
    let mut entities_processed = 0;
    for scenario in test_scenarios.iter() {
        if let Ok(result) = knowledge_scribe.link_data_to_graph(scenario).await {
            debug!("link_data_to_graph result: {}", result);
            entities_processed += 1;
        }
    }
    Ok(json!({ "entities_processed": entities_processed }))
}
