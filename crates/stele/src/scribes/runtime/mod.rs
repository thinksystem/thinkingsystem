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



use crate::database::structured_store::StructuredStore;
use crate::scribes::specialists::{DataScribe, IdentityScribe, KnowledgeScribe};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScribeEvent {
    NewUtterance {
        utterance_id: String,
        user_id: String,
        channel: String,
        text: String,
    },
    ExtractionComplete {
        utterance_id: String,
        nodes: usize,
        relationships: usize,
    },
    IdentityResolved {
        user_id: String,
        trust_score: f32,
    },
    KnowledgeUpdated {
        links: usize,
        merges: usize,
    },
    Error {
        stage: String,
        detail: String,
    },
}




#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedTask {
    pub utterance_id: String,
    pub user_id: String,
    pub channel: String,
    pub raw_text: String,
    pub entities: Vec<String>,
    pub intents: Vec<String>,
    pub knowledge_context: Vec<String>,
}

pub struct ScribeRuntimeManager {
    #[allow(dead_code)]
    identity: Arc<RwLock<IdentityScribe>>,
    #[allow(dead_code)]
    data: Arc<RwLock<DataScribe>>,
    #[allow(dead_code)]
    knowledge: Arc<RwLock<KnowledgeScribe>>,
    tx: UnboundedSender<ScribeEvent>,
    #[allow(dead_code)]
    rx: UnboundedReceiver<ScribeEvent>,
    canonical_store: tokio::sync::Mutex<Option<StructuredStore>>,
}

impl ScribeRuntimeManager {
    pub async fn new(
        identity: IdentityScribe,
        data: DataScribe,
        knowledge: KnowledgeScribe,
    ) -> Result<Self, String> {
        let (tx, rx) = unbounded_channel();
        Ok(Self {
            identity: Arc::new(RwLock::new(identity)),
            data: Arc::new(RwLock::new(data)),
            knowledge: Arc::new(RwLock::new(knowledge)),
            tx,
            rx,
            canonical_store: tokio::sync::Mutex::new(None),
        })
    }
    pub fn event_sender(&self) -> UnboundedSender<ScribeEvent> {
        self.tx.clone()
    }
    pub async fn handle_message(
        &self,
        user_id: &str,
        channel: &str,
        text: &str,
    ) -> Result<Value, String> {
        let utterance_id = uuid::Uuid::new_v4().to_string();
        let _ = self.tx.send(ScribeEvent::NewUtterance {
            utterance_id: utterance_id.clone(),
            user_id: user_id.to_string(),
            channel: channel.to_string(),
            text: text.to_string(),
        });
        
        let identity_score = 0.9_f32; 
        let _ = self.tx.send(ScribeEvent::IdentityResolved {
            user_id: user_id.to_string(),
            trust_score: identity_score,
        });
        
        {
            let mut guard = self.canonical_store.lock().await;
            if guard.is_none() {
                if let Ok(canon) = StructuredStore::connect_canonical_from_env().await {
                    
                    let store = StructuredStore::new_with_clients(canon.clone(), canon, true);
                    *guard = Some(store);
                }
            }
            if let Some(store) = guard.as_ref() {
                let _ = store
                    .upsert_canonical_entity(
                        "Identity",
                        user_id,
                        Some(&format!("identity:{user_id}")),
                        Some(
                            serde_json::json!({"channel": channel, "trust_score": identity_score}),
                        ),
                    )
                    .await;
            }
        }
        
        let mut data_lock = self.data.write().await;
        let extracted = data_lock
            .process_and_store(&serde_json::json!({"text": text}), user_id, channel)
            .await?;
        let nodes = extracted
            .get("extracted_data")
            .and_then(|v| v.get("nodes"))
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let relationships = extracted
            .get("extracted_data")
            .and_then(|v| v.get("relationships"))
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let _ = self.tx.send(ScribeEvent::ExtractionComplete {
            utterance_id: utterance_id.clone(),
            nodes,
            relationships,
        });
        
        let _ = self.tx.send(ScribeEvent::KnowledgeUpdated {
            links: 0,
            merges: 0,
        });
        Ok(
            serde_json::json!({"utterance_id": utterance_id, "nodes": nodes, "relationships": relationships}),
        )
    }

    
    
    pub async fn prepare_task(
        &self,
        user_id: &str,
        channel: &str,
        text: &str,
    ) -> Result<PreparedTask, String> {
        tracing::info!(target: "stele::runtime", user_id=%user_id, channel=%channel, len=text.len(), "prepare_task.start");
        let mut data_lock = self.data.write().await;
        let processed = data_lock
            .process_and_store(&serde_json::json!({"text": text}), user_id, channel)
            .await?;
        
        let mut entities: Vec<String> = Vec::new();
        let mut intents: Vec<String> = Vec::new();
        if let Some(extracted) = processed.get("extracted_data") {
            if let Some(nodes) = extracted.get("nodes").and_then(|v| v.as_array()) {
                for n in nodes.iter().take(32) {
                    if let Some(name) = n.get("name").and_then(|v| v.as_str()) {
                        entities.push(name.to_string());
                    } else if let Some(eobj) = n.get("Entity").or_else(|| n.get("entity")) {
                        if let Some(name) = eobj.get("name").and_then(|v| v.as_str()) {
                            entities.push(name.to_string());
                        }
                    }
                }
            }
        }
        if let Some(segments) = processed.get("segments").and_then(|v| v.as_array()) {
            for seg in segments.iter().take(16) {
                if let Some(intent) = seg.get("intent").and_then(|v| v.as_str()) {
                    intents.push(intent.to_string());
                } else if let Some(st) = seg.get("segment_type").and_then(|v| v.as_str()) {
                    if st.to_lowercase().contains("intent") {
                        intents.push(st.to_string());
                    }
                }
            }
        }
        
        let mut knowledge_ctx: Vec<String> = Vec::new();
        if let Ok(klock) = self.knowledge.try_write() {
            for e in entities.iter().take(5) {
                if let Some(data) = klock.get_entity_data(e) {
                    knowledge_ctx.push(format!("{e}: {data}"));
                }
            }
        }
        
        let _ = self.tx.send(ScribeEvent::NewUtterance {
            utterance_id: "prepared-only".into(),
            user_id: user_id.to_string(),
            channel: channel.to_string(),
            text: text.to_string(),
        });
        Ok(PreparedTask {
            utterance_id: processed
                .get("utterance_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            user_id: user_id.to_string(),
            channel: channel.to_string(),
            raw_text: text.to_string(),
            entities,
            intents,
            knowledge_context: knowledge_ctx,
        })
    }

    

    
    
    pub fn apply_reward(&self, reward: f32) {
        if let Ok(mut d) = self.data.try_write() {
            d.record_reward(reward);
        }
        if let Ok(mut k) = self.knowledge.try_write() {
            k.record_reward(reward);
        }
        if let Ok(mut i) = self.identity.try_write() {
            i.record_reward(reward);
        }
    }
}


