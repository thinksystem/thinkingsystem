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
use surrealdb::sql::{Datetime, Thing};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EventNode {
    #[serde(default = "default_thing")]
    pub id: Thing,
    pub name: String,
    pub start_time: Datetime,
    pub end_time: Datetime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EventEdge {
    #[serde(default = "default_thing")]
    pub id: Thing,
    pub relationship: String,
    pub weight: f32,
}

impl Record for EventNode {
    const TABLE_NAME: &'static str = "events";
}
impl Node for EventNode {
    fn get_id(&self) -> &Thing {
        &self.id
    }
}
impl Record for EventEdge {
    const TABLE_NAME: &'static str = "event_links";
}
impl Edge for EventEdge {}

impl GraphClient<EventNode, EventEdge> {
    pub async fn init_schema(&self) -> Result<(), surrealdb::Error> {
        let schema = r#"
            DEFINE TABLE events SCHEMAFULL;
            DEFINE FIELD name ON events TYPE string;
            DEFINE FIELD start_time ON events TYPE datetime;
            DEFINE FIELD end_time ON events TYPE datetime;
            DEFINE INDEX event_time ON events COLUMNS start_time, end_time;
            DEFINE TABLE event_links SCHEMAFULL TYPE RELATION IN events OUT events;
            DEFINE FIELD relationship ON event_links TYPE string;
            DEFINE FIELD weight ON event_links TYPE float;
        "#;
        self.define(schema).await
    }

    pub async fn find_events_in_range(
        &self,
        start: Datetime,
        end: Datetime,
    ) -> Result<Vec<EventNode>, surrealdb::Error> {
        let start_str = format!("{start}").replace("d'", "").replace("'", "");
        let end_str = format!("{end}").replace("d'", "").replace("'", "");

        let query = format!("SELECT * FROM events WHERE start_time >= d'{start_str}' AND start_time <= d'{end_str}'");
        self.query(&query).execute().await
    }

    pub async fn find_overlapping_events(
        &self,
        event_id: &Thing,
    ) -> Result<Vec<EventNode>, surrealdb::Error> {
        let query = format!("SELECT * FROM events WHERE id = {event_id}");
        let target_events: Vec<EventNode> = self.query(&query).execute().await?;

        if let Some(event) = target_events.first() {
            let end_time_str = format!("{}", event.end_time)
                .replace("d'", "")
                .replace("'", "");
            let start_time_str = format!("{}", event.start_time)
                .replace("d'", "")
                .replace("'", "");

            let query = format!("SELECT * FROM events WHERE id != {event_id} AND (start_time < d'{end_time_str}' AND end_time > d'{start_time_str}')");
            self.query(&query).execute().await
        } else {
            Ok(vec![])
        }
    }
}
