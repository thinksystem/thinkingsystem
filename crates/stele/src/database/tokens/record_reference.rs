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

use crate::database::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordReferenceToken {
    Source(ReferenceSourceConfig),
    Target(ReferenceTargetConfig),
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReferenceSourceConfig {
    pub on_delete: OnDeleteBehaviour,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReferenceTargetConfig {
    pub from_table: Option<String>,
    pub from_field: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnDeleteBehaviour {
    Ignore,
    Unset,
    Cascade,
    Reject,
    Then(String),
}
impl Default for OnDeleteBehaviour {
    fn default() -> Self {
        Self::Ignore
    }
}
impl ReferenceSourceConfig {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn on_delete(mut self, behaviour: OnDeleteBehaviour) -> Self {
        self.on_delete = behaviour;
        self
    }
}
impl ReferenceTargetConfig {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn from_table(mut self, table: String) -> Self {
        self.from_table = Some(table);
        self
    }
    pub fn from_field(mut self, table: String, field: String) -> Self {
        self.from_table = Some(table);
        self.from_field = Some(field);
        self
    }
}
