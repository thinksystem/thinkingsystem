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

pub mod definition {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use std::collections::HashMap;
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum BlockType {
        Conditional {
            condition: String,
            true_block: String,
            false_block: String,
        },
        Compute {
            expression: String,
            output_key: String,
            next_block: String,
        },
        AwaitInput {
            interaction_id: String,
            agent_id: String,
            prompt: String,
            state_key: String,
            next_block: String,
        },
        ForEach {
            loop_id: String,
            array_path: String,
            iterator_var: String,
            loop_body_block_id: String,
            exit_block_id: String,
        },
        TryCatch {
            try_block_id: String,
            catch_block_id: String,
        },
        SubFlow {
            flow_id: String,
            input_map: HashMap<String, Value>,
            output_key: String,
            next_block: String,
        },
        Continue {
            loop_id: String,
        },
        Break {
            loop_id: String,
        },
        Terminate,
    }
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BlockDefinition {
        pub id: String,
        pub block_type: BlockType,
    }
    impl BlockDefinition {
        pub fn new(id: impl Into<String>, block_type: BlockType) -> Self {
            Self {
                id: id.into(),
                block_type,
            }
        }
    }
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FlowDefinition {
        pub id: String,
        pub start_block_id: String,
        pub blocks: Vec<BlockDefinition>,
        pub participants: Vec<String>,
        pub permissions: HashMap<String, Vec<String>>,
        pub initial_state: Option<Value>,
        pub state_schema: Option<Value>,
    }
    impl FlowDefinition {
        pub fn new(id: impl Into<String>, start_block_id: impl Into<String>) -> Self {
            Self {
                id: id.into(),
                start_block_id: start_block_id.into(),
                blocks: Vec::new(),
                participants: Vec::new(),
                permissions: HashMap::new(),
                initial_state: None,
                state_schema: None,
            }
        }
        pub fn add_block(&mut self, block: BlockDefinition) -> &mut Self {
            self.blocks.push(block);
            self
        }
        pub fn add_participant(&mut self, participant: impl Into<String>) -> &mut Self {
            self.participants.push(participant.into());
            self
        }
        pub fn set_initial_state(&mut self, state: Value) -> &mut Self {
            self.initial_state = Some(state);
            self
        }
        pub fn set_state_schema(&mut self, schema: Value) -> &mut Self {
            self.state_schema = Some(schema);
            self
        }
        pub fn get_block(&self, id: &str) -> Option<&BlockDefinition> {
            self.blocks.iter().find(|b| b.id == id)
        }
    }
}
pub use definition::*;
