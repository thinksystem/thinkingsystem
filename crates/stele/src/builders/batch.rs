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


use super::relationship::RelationshipStrategy;
use super::{NodeBuilder, PendingNode, PendingRelationship, RelationshipBuilder};
#[cfg(feature = "hybrid_relationships")]
use crate::relationships::hybrid::persist_hybrid_relationship;
use crate::StructuredStore;
use std::collections::HashMap;
use surrealdb::sql::Thing;

#[derive(Debug)]
pub struct IngestResult {
    pub node_map: HashMap<String, Thing>,
    
    pub relationships: Vec<serde_json::Value>,
    #[cfg(feature = "api_v2")]
    pub api_v2: Option<serde_json::Value>,
}

#[derive(Default)]
pub struct IngestBatchBuilder {
    nodes: Vec<PendingNode>,
    rels: Vec<PendingRelationship>,
}

impl IngestBatchBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add_node(mut self, nb: NodeBuilder) -> Self {
        self.nodes.push(nb.build());
        self
    }
    pub fn add_relationship(mut self, rb: RelationshipBuilder) -> Self {
        self.rels.push(rb.build());
        self
    }
    pub async fn execute(self, store: &StructuredStore) -> anyhow::Result<IngestResult> {
        
        let mut node_map = HashMap::new();
        for n in &self.nodes {
            let name = n.name.clone().unwrap_or_else(|| "node".into());
            let et = n.entity_type.clone().unwrap_or_else(|| "generic".into());
            
            let key = format!("{et}:{name}");
            let id = store
                .upsert_canonical_entity(&et, &name, Some(&key), n.extra.clone())
                .await?;
            if let Some(temp) = &n.temp_id {
                node_map.insert(temp.clone(), id);
            }
        }
        
        let mut rel_results = Vec::new();
        for r in &self.rels {
            
            let resolved_source = if let Some(id) = &r.source_id {
                Some(id.clone())
            } else if let Some(temp) = &r.source_temp {
                node_map.get(temp).cloned()
            } else {
                None
            };
            let resolved_target = if let Some(id) = &r.target_id {
                Some(id.clone())
            } else if let Some(temp) = &r.target_temp {
                node_map.get(temp).cloned()
            } else {
                None
            };
            if let (Some(sid), Some(tid), Some(pred)) = (
                resolved_source.as_ref(),
                resolved_target.as_ref(),
                &r.predicate,
            ) {
                let prov_str = r.provenance.as_ref().map(|v| v.to_string());
                let result_json = match r.strategy {
                    RelationshipStrategy::EdgeOnly => {
                        match store
                            .create_relationship_fact(
                                sid,
                                pred,
                                tid,
                                r.confidence,
                                prov_str.as_deref(),
                            )
                            .await
                        {
                            Ok(id) => {
                                serde_json::json!({"predicate": pred, "fact_id": id.to_string(), "strategy": "EdgeOnly"})
                            }
                            Err(e) => {
                                serde_json::json!({"predicate": pred, "error": e.to_string(), "strategy": "EdgeOnly"})
                            }
                        }
                    }
                    #[cfg(feature = "hybrid_relationships")]
                    RelationshipStrategy::Hybrid => {
                        match persist_hybrid_relationship(store, r, sid, tid).await {
                            Ok(res) => {
                                
                                let virtual_node = if res.node_id.is_none() {
                                    crate::relationships::metrics::record_virtual_node();
                                    Some(serde_json::json!({
                                        "subject_ref": sid.to_string(),
                                        "object_ref": tid.to_string(),
                                        "predicate": pred,
                                        "confidence": r.confidence,
                                        "provenance": r.provenance.clone().unwrap_or_else(|| serde_json::json!({})),
                                        "synthetic": true
                                    }))
                                } else {
                                    None
                                };
                                serde_json::json!({
                                    "predicate": pred,
                                    "fact_id": res.fact_id.map(|t| t.to_string()),
                                    "node_id": res.node_id.map(|t| t.to_string()),
                                    "degraded": res.degraded,
                                    "strategy": "Hybrid",
                                    "virtual_node": virtual_node
                                })
                            }
                            Err(e) => {
                                serde_json::json!({"predicate": pred, "error": e.to_string(), "strategy": "Hybrid"})
                            }
                        }
                    }
                };
                rel_results.push(result_json);
            }
        }
        #[cfg(feature = "api_v2")]
        let api_v2_payload = Some(serde_json::json!({
            "api_version": "v2",
            "nodes": node_map.iter().map(|(k,v)| serde_json::json!({"temp_id": k, "id": v.to_string()})).collect::<Vec<_>>(),
            "relationships": rel_results,
        }));
        #[cfg(not(feature = "api_v2"))]
        let api_v2_payload = ();
        Ok(IngestResult {
            node_map,
            relationships: rel_results,
            #[cfg(feature = "api_v2")]
            api_v2: api_v2_payload,
        })
    }
}
