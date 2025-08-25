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
use std::sync::Arc;
use tokio::sync::RwLock;
use once_cell::sync::OnceCell;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProvExecMeta {
    pub session_id: Option<String>,
    pub flow_id: Option<String>,
    pub theatre_id: Option<String>,
    pub block_ids: Vec<String>,
}

#[derive(Clone, Default)]
pub struct ProvenanceContext(Arc<RwLock<ProvExecMeta>>);

impl ProvenanceContext {
    pub fn new() -> Self {
        Self::default()
    }
    pub async fn set_session(&self, session_id: &str, flow_id: Option<&str>) {
        let mut g = self.0.write().await;
        g.session_id = Some(session_id.to_string());
        g.flow_id = flow_id.map(|s| s.to_string());
    }
    pub async fn set_theatre(&self, theatre_id: &str) {
        let mut g = self.0.write().await;
        g.theatre_id = Some(theatre_id.to_string());
    }
    pub async fn push_block(&self, block_id: &str) {
        let mut g = self.0.write().await;
        g.block_ids.push(block_id.to_string());
    }
    pub async fn snapshot(&self) -> ProvExecMeta {
        self.0.read().await.clone()
    }
}



static GLOBAL_CONTEXT: OnceCell<ProvenanceContext> = OnceCell::new();

pub fn global() -> ProvenanceContext {
    GLOBAL_CONTEXT
    .get_or_init(ProvenanceContext::new)
        .clone()
}
