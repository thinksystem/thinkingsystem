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

pub mod data_scribe;
pub mod identity_scribe;
pub mod knowledge_scribe;
pub use data_scribe::DataScribe;
pub use identity_scribe::IdentityScribe;
pub use knowledge_scribe::KnowledgeScribe;
pub type ScribeId = String;
#[derive(Clone)]
pub enum Scribe {
    Data(DataScribe),
    Knowledge(KnowledgeScribe),
    Identity(IdentityScribe),
}
impl Scribe {
    pub fn id(&self) -> ScribeId {
        match self {
            Scribe::Data(s) => s.id.clone(),
            Scribe::Knowledge(s) => s.id.clone(),
            Scribe::Identity(s) => s.id.clone(),
        }
    }
}

impl std::fmt::Debug for Scribe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scribe::Data(s) => f.debug_struct("Scribe::Data").field("id", &s.id).finish(),
            Scribe::Knowledge(s) => f
                .debug_struct("Scribe::Knowledge")
                .field("id", &s.id)
                .finish(),
            Scribe::Identity(s) => f
                .debug_struct("Scribe::Identity")
                .field("id", &s.id)
                .finish(),
        }
    }
}
