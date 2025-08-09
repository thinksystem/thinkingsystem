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

use crate::graphs::core::{Edge, GraphClient, Node};
use async_trait::async_trait;
use surrealdb::sql::{Thing, Value as SurrealValue};

#[async_trait]
pub trait PathfindingStrategy<N: Node, E: Edge>: Send + Sync {
    async fn find_path(
        &self,
        client: &GraphClient<N, E>,
        from: &Thing,
        to: &Thing,
    ) -> Result<Vec<SurrealValue>, surrealdb::Error>;
}

pub struct ManualPathfinder;

#[async_trait]
impl<N: Node, E: Edge> PathfindingStrategy<N, E> for ManualPathfinder {
    async fn find_path(
        &self,
        client: &GraphClient<N, E>,
        from: &Thing,
        to: &Thing,
    ) -> Result<Vec<SurrealValue>, surrealdb::Error> {
        let direct_sql = format!(
            "SELECT count() FROM {} WHERE in = {} AND out = {}",
            E::TABLE_NAME,
            from,
            to
        );
        let direct_result = client
            .query(&direct_sql)
            .execute::<serde_json::Value>()
            .await?;

        let direct_count = if let Some(first) = direct_result.first() {
            if let Some(count) = first.get("count") {
                count.as_i64().unwrap_or(0)
            } else {
                0
            }
        } else {
            0
        };

        if direct_count > 0 {
            return Ok(vec![
                SurrealValue::from(from.clone()),
                SurrealValue::from(to.clone()),
            ]);
        }

        let intermediate_sql = format!(
            "SELECT VALUE out FROM {} WHERE in = {}",
            E::TABLE_NAME,
            from
        );
        let intermediate_nodes: Vec<Thing> = client.query(&intermediate_sql).execute().await?;

        for intermediate in &intermediate_nodes {
            let path_sql = format!(
                "SELECT count() FROM {} WHERE in = {} AND out = {}",
                E::TABLE_NAME,
                intermediate,
                to
            );
            let path_result = client
                .query(&path_sql)
                .execute::<serde_json::Value>()
                .await?;

            let path_count = if let Some(first) = path_result.first() {
                if let Some(count) = first.get("count") {
                    count.as_i64().unwrap_or(0)
                } else {
                    0
                }
            } else {
                0
            };

            if path_count > 0 {
                return Ok(vec![
                    SurrealValue::from(from.clone()),
                    SurrealValue::from(intermediate.clone()),
                    SurrealValue::from(to.clone()),
                ]);
            }
        }

        Ok(vec![])
    }
}

pub struct NativeGraphRoute;

#[async_trait]
impl<N: Node, E: Edge> PathfindingStrategy<N, E> for NativeGraphRoute {
    async fn find_path(
        &self,
        client: &GraphClient<N, E>,
        from: &Thing,
        to: &Thing,
    ) -> Result<Vec<SurrealValue>, surrealdb::Error> {
        let sql = "SELECT * FROM graph::route($from, $to)";
        client
            .query(sql)
            .bind("from", from.clone())
            .bind("to", to.clone())
            .execute()
            .await
    }
}
