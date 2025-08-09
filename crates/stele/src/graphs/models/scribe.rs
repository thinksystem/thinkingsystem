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

use crate::graphs::core::{Edge, GraphClient, Node, Record};
use crate::graphs::models::default_thing;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use surrealdb::sql::Thing;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScribeNode {
    #[serde(default = "default_thing")]
    pub id: Thing,
    pub name: String,
    pub coords: [f32; 2],
    pub specialisation: HashMap<String, f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScribeEdge {
    #[serde(default = "default_thing")]
    pub id: Thing,
    pub weight: f32,
}

impl Record for ScribeNode {
    const TABLE_NAME: &'static str = "scribes";
}
impl Node for ScribeNode {
    fn get_id(&self) -> &Thing {
        &self.id
    }
}
impl Record for ScribeEdge {
    const TABLE_NAME: &'static str = "routes";
}
impl Edge for ScribeEdge {}

impl GraphClient<ScribeNode, ScribeEdge> {
    pub async fn init_schema(&self) -> Result<(), surrealdb::Error> {
        let schema = r#"
            DEFINE TABLE scribes SCHEMAFULL;
            DEFINE FIELD name ON scribes TYPE string;
            DEFINE FIELD coords ON scribes TYPE array<float>;
            DEFINE FIELD specialisation ON scribes TYPE object;
            DEFINE TABLE routes SCHEMAFULL TYPE RELATION IN scribes OUT scribes;
            DEFINE FIELD weight ON routes TYPE float;
        "#;
        self.define(schema).await
    }

    pub async fn set_scribe_coords(
        &self,
        scribe_id: &Thing,
        new_coords: [f32; 2],
    ) -> Result<Option<ScribeNode>, surrealdb::Error> {
        let sql = format!("UPDATE {scribe_id} MERGE $data");
        let mut results: Vec<ScribeNode> = self
            .query(&sql)
            .bind("data", serde_json::json!({ "coords": new_coords }))
            .execute()
            .await?;
        Ok(results.pop())
    }

    pub async fn find_scribes_within_radius(
        &self,
        centre: [f32; 2],
        radius: f32,
    ) -> Result<Vec<ScribeNode>, surrealdb::Error> {
        let sql = format!(
            "SELECT * FROM scribes WHERE math::sqrt(math::pow(coords[0] - {}, 2) + math::pow(coords[1] - {}, 2)) <= {}",
            centre[0], centre[1], radius
        );
        self.query(&sql).execute().await
    }

    pub async fn find_nearest_scribe(
        &self,
        centre: [f32; 2],
    ) -> Result<Option<ScribeNode>, surrealdb::Error> {
        let sql = format!(
            "SELECT *, math::sqrt(math::pow(coords[0] - {}, 2) + math::pow(coords[1] - {}, 2)) AS distance FROM scribes ORDER BY distance ASC LIMIT 1",
            centre[0], centre[1]
        );
        let mut results: Vec<ScribeNode> = self.query(&sql).execute().await?;
        Ok(results.pop())
    }

    pub async fn update_specialisation(
        &self,
        scribe_id: &Thing,
        skill: String,
        value: f32,
    ) -> Result<Option<ScribeNode>, surrealdb::Error> {
        let sql = format!("UPDATE {scribe_id} MERGE $data");
        let data = serde_json::json!({
            "specialisation": {
                skill: value
            }
        });
        let mut results: Vec<ScribeNode> = self.query(&sql).bind("data", data).execute().await?;
        Ok(results.pop())
    }
}
