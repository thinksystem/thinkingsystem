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

#[derive(Debug, Clone)]
pub struct PendingNode {
    pub temp_id: Option<String>,
    pub entity_type: Option<String>,
    pub name: Option<String>,
    pub extra: Option<Value>,
}

#[derive(Debug, Default, Clone)]
pub struct NodeBuilder {
    temp_id: Option<String>,
    entity_type: Option<String>,
    name: Option<String>,
    extra: Option<Value>,
}

impl NodeBuilder {
    pub fn new() -> Self { Self::default() }
    pub fn temp_id(mut self, id: impl Into<String>) -> Self { self.temp_id = Some(id.into()); self }
    pub fn entity_type(mut self, et: impl Into<String>) -> Self { self.entity_type = Some(et.into()); self }
    pub fn name(mut self, n: impl Into<String>) -> Self { self.name = Some(n.into()); self }
    pub fn extra(mut self, v: Value) -> Self { self.extra = Some(v); self }
    pub fn build(self) -> PendingNode { PendingNode { temp_id: self.temp_id, entity_type: self.entity_type, name: self.name, extra: self.extra } }
}
