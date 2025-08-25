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


use serde_json::Value;
use surrealdb::sql::Thing;

#[derive(Debug, Clone, Copy, Default)]
pub enum RelationshipStrategy {
    #[default]
    EdgeOnly,
    #[cfg(feature = "hybrid_relationships")]
    Hybrid,
}

#[derive(Debug, Clone)]
pub struct PendingRelationship {
    pub source_temp: Option<String>,
    pub target_temp: Option<String>,
    pub source_id: Option<Thing>,
    pub target_id: Option<Thing>,
    pub predicate: Option<String>,
    pub confidence: Option<f32>,
    pub provenance: Option<Value>,
    pub strategy: RelationshipStrategy,
}

#[derive(Debug, Default, Clone)]
pub struct RelationshipBuilder {
    source_temp: Option<String>,
    target_temp: Option<String>,
    source_id: Option<Thing>,
    target_id: Option<Thing>,
    predicate: Option<String>,
    confidence: Option<f32>,
    provenance: Option<Value>,
    strategy: RelationshipStrategy,
}

impl RelationshipBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn source_temp(mut self, id: impl Into<String>) -> Self {
        self.source_temp = Some(id.into());
        self
    }
    pub fn target_temp(mut self, id: impl Into<String>) -> Self {
        self.target_temp = Some(id.into());
        self
    }
    pub fn source_id(mut self, id: Thing) -> Self {
        self.source_id = Some(id);
        self
    }
    
    pub fn source_id_str(mut self, id: &str) -> Self {
        if let Ok(t) = id.parse::<Thing>() {
            self.source_id = Some(t);
        }
        self
    }
    pub fn target_id(mut self, id: Thing) -> Self {
        self.target_id = Some(id);
        self
    }
    
    pub fn target_id_str(mut self, id: &str) -> Self {
        if let Ok(t) = id.parse::<Thing>() {
            self.target_id = Some(t);
        }
        self
    }
    pub fn predicate(mut self, p: impl Into<String>) -> Self {
        self.predicate = Some(p.into());
        self
    }
    pub fn confidence(mut self, c: f32) -> Self {
        self.confidence = Some(c);
        self
    }
    pub fn provenance(mut self, v: Value) -> Self {
        self.provenance = Some(v);
        self
    }
    pub fn strategy(mut self, s: RelationshipStrategy) -> Self {
        self.strategy = s;
        self
    }
    pub fn build(self) -> PendingRelationship {
        PendingRelationship {
            source_temp: self.source_temp,
            target_temp: self.target_temp,
            source_id: self.source_id,
            target_id: self.target_id,
            predicate: self.predicate,
            confidence: self.confidence,
            provenance: self.provenance,
            strategy: self.strategy,
        }
    }
}
