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

use crate::blocks::registry::BlockRegistry;
use crate::blocks::rules::BlockError;
use crate::nlu::orchestrator::ExtractedData;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

pub struct LLMPromptService {
    registry: Arc<BlockRegistry>,
    block_descriptions: HashMap<String, String>,
}

impl LLMPromptService {
    pub fn new(registry: Arc<BlockRegistry>) -> Self {
        Self {
            registry,
            block_descriptions: Self::create_default_block_descriptions(),
        }
    }

    fn create_default_block_descriptions() -> HashMap<String, String> {
        let mut descriptions = HashMap::new();

        descriptions.insert(
            "conditional".to_string(),
            "Evaluates if/then conditions, branches flow logic".to_string(),
        );
        descriptions.insert(
            "decision".to_string(),
            "Makes routing decisions based on true/false evaluation".to_string(),
        );
        descriptions.insert(
            "display".to_string(),
            "Shows messages, outputs text to user interface".to_string(),
        );
        descriptions.insert(
            "external_data".to_string(),
            "Fetches data from APIs and external sources".to_string(),
        );
        descriptions.insert(
            "goto".to_string(),
            "Unconditionally jumps to another block in flow".to_string(),
        );
        descriptions.insert(
            "input".to_string(),
            "Pauses for user input, collects data interactively".to_string(),
        );
        descriptions.insert(
            "interactive".to_string(),
            "Presents choices, waits for user selection".to_string(),
        );
        descriptions.insert(
            "random".to_string(),
            "Randomly selects path using weighted probabilities".to_string(),
        );
        descriptions.insert(
            "compute".to_string(),
            "Performs calculations, string operations, data transformations".to_string(),
        );
        descriptions.insert(
            "terminal".to_string(),
            "Ends flow execution, final termination block".to_string(),
        );

        descriptions
    }

    pub fn generate_flow_logic_prompt(&self, data: &ExtractedData) -> Result<String, BlockError> {
        debug!("Generating dynamic flow logic prompt from extracted data");

        let actions: Vec<_> = data.actions().collect();
        let entities: Vec<_> = data.entities().collect();
        let temporal_markers: Vec<_> = data.temporal_markers().collect();

        let available_blocks = self.registry.get_available_block_types()?;
        let block_descriptions = self.build_dynamic_block_descriptions(&available_blocks);

        let prompt = self.build_structured_prompt(
            &actions,
            &entities,
            &temporal_markers,
            &data.relationships,
            &block_descriptions,
        );

        debug!(
            "Generated dynamic prompt with {} available block types",
            available_blocks.len()
        );
        Ok(prompt)
    }

    fn build_structured_prompt(
        &self,
        actions: &[&crate::nlu::orchestrator::data_models::Action],
        entities: &[&crate::nlu::orchestrator::data_models::Entity],
        temporal_markers: &[&crate::nlu::orchestrator::data_models::TemporalMarker],
        relationships: &[crate::nlu::orchestrator::data_models::Relationship],
        block_descriptions: &str,
    ) -> String {
        format!(
            "# Flow Generation Task\n\n\
            Create a JSON workflow definition based on the following analysis:\n\n\
            ## Extracted Components\n\
            **Actions:** {}\n\
            **Entities:** {}\n\
            **Temporal Elements:** {}\n\
            **Relationships:** {}\n\n\
            ## Available Block Types\n\
            {}\n\n\
            ## Required JSON Structure\n\
            ```json\n\
            {{\n\
              \"id\": \"unique_flow_id\",\n\
              \"name\": \"Descriptive Flow Name\",\n\
              \"start_block_id\": \"start_block\",\n\
              \"blocks\": [\n\
                {{\n\
                  \"id\": \"block_id\",\n\
                  \"type\": \"BlockType\",\n\
                  \"properties\": {{\n\
                    \"key\": \"value\"\n\
                  }}\n\
                }}\n\
              ]\n\
            }}\n\
            ```\n\n\
            **Instructions:**\n\
            - Generate valid JSON only (no markdown wrapper)\n\
            - Use meaningful block IDs and flow names\n\
            - Choose appropriate block types from the available list\n\
            - Ensure proper flow connectivity between blocks",
            self.format_actions(actions),
            self.format_entities(entities),
            self.format_temporal_markers(temporal_markers),
            self.format_relationships(relationships),
            block_descriptions
        )
    }

    fn format_actions(&self, actions: &[&crate::nlu::orchestrator::data_models::Action]) -> String {
        if actions.is_empty() {
            "None".to_string()
        } else {
            actions
                .iter()
                .map(|action| action.verb.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    fn format_entities(
        &self,
        entities: &[&crate::nlu::orchestrator::data_models::Entity],
    ) -> String {
        if entities.is_empty() {
            "None".to_string()
        } else {
            entities
                .iter()
                .map(|entity| entity.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    fn format_temporal_markers(
        &self,
        temporal_markers: &[&crate::nlu::orchestrator::data_models::TemporalMarker],
    ) -> String {
        if temporal_markers.is_empty() {
            "None".to_string()
        } else {
            temporal_markers
                .iter()
                .map(|marker| marker.date_text.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    fn format_relationships(
        &self,
        relationships: &[crate::nlu::orchestrator::data_models::Relationship],
    ) -> String {
        if relationships.is_empty() {
            "None".to_string()
        } else {
            relationships
                .iter()
                .map(|rel| format!("{} {} {}", rel.source, rel.relation_type, rel.target))
                .collect::<Vec<_>>()
                .join("; ")
        }
    }

    fn build_dynamic_block_descriptions(&self, available_blocks: &[String]) -> String {
        let mut descriptions = Vec::new();

        for block_type in available_blocks {
            let description = self
                .block_descriptions
                .get(block_type)
                .map(|d| d.as_str())
                .unwrap_or("Generic block type");

            descriptions.push(format!(
                "• {} - {}",
                self.format_block_name(block_type),
                description
            ));
        }

        descriptions.join("\n")
    }

    fn format_block_name(&self, block_type: &str) -> String {
        block_type
            .split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                }
            })
            .collect::<Vec<String>>()
            .join("")
    }

    pub fn register_block_description(&mut self, block_type: String, description: String) {
        debug!("Registering description for block type: {}", block_type);
        self.block_descriptions.insert(block_type, description);
    }

    pub fn get_available_block_types(&self) -> Result<Vec<String>, BlockError> {
        self.registry.get_available_block_types()
    }

    pub fn is_block_type_available(&self, block_type: &str) -> Result<bool, BlockError> {
        let available = self.registry.get_available_block_types()?;
        Ok(available.contains(&block_type.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::registry::BlockRegistry;

    #[test]
    fn test_format_block_name() {
        let registry = Arc::new(BlockRegistry::new());
        let service = LLMPromptService::new(registry);

        assert_eq!(service.format_block_name("external_data"), "ExternalData");
        assert_eq!(service.format_block_name("compute"), "Compute");
        assert_eq!(
            service.format_block_name("dynamic_function"),
            "DynamicFunction"
        );
    }

    #[tokio::test]
    async fn test_dynamic_block_descriptions() -> Result<(), BlockError> {
        let registry = Arc::new(BlockRegistry::with_standard_blocks()?);
        let service = LLMPromptService::new(registry);

        let available_blocks = service.get_available_block_types()?;
        assert!(!available_blocks.is_empty());
        assert!(available_blocks.contains(&"conditional".to_string()));
        assert!(available_blocks.contains(&"external_data".to_string()));

        Ok(())
    }
}
