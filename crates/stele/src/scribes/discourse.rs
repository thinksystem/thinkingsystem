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

use super::specialists::ScribeId;
use serde_json::Value;
#[derive(Debug, Clone)]
pub struct Inscription {
    pub scribe_id: ScribeId,
    pub action: String,
    pub result: Result<Value, String>,
}
#[derive(Debug, Clone)]
pub struct Testament {
    pub canon_invoked: String,
    pub participants: Vec<ScribeId>,
    pub was_successful: bool,
    pub final_product: Value,
    pub chronicle: Vec<Inscription>,
}
pub enum DiscourseState {
    AwaitingAction {
        scribe_id: ScribeId,
        action_name: String,
        context: Value,
    },
    Concluded(Testament),
}
