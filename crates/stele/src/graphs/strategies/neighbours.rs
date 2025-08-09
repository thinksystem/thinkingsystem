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
use surrealdb::sql::Thing;

#[async_trait]
pub trait NeighbourStrategy<N: Node, E: Edge>: Send + Sync {
    async fn get_neighbours(
        &self,
        client: &GraphClient<N, E>,
        node_id: &Thing,
    ) -> Result<Vec<N>, surrealdb::Error>;
}

pub struct ManualNeighbours;

#[async_trait]
impl<N: Node, E: Edge> NeighbourStrategy<N, E> for ManualNeighbours {
    async fn get_neighbours(
        &self,
        client: &GraphClient<N, E>,
        node_id: &Thing,
    ) -> Result<Vec<N>, surrealdb::Error> {
        let sql = format!(
            "SELECT VALUE out FROM {} WHERE in = {}",
            E::TABLE_NAME,
            node_id
        );

        let neighbour_ids: Vec<Thing> = client.query(&sql).execute().await?;

        let mut neighbours = Vec::new();
        for id in neighbour_ids {
            if let Some(node) = client.get_node(&id).await? {
                neighbours.push(node);
            }
        }

        Ok(neighbours)
    }
}

pub struct FetchNeighbours;

#[async_trait]
impl<N: Node, E: Edge> NeighbourStrategy<N, E> for FetchNeighbours {
    async fn get_neighbours(
        &self,
        client: &GraphClient<N, E>,
        node_id: &Thing,
    ) -> Result<Vec<N>, surrealdb::Error> {
        let sql = format!(
            "SELECT VALUE ->{}->{}.*  FROM {}",
            E::TABLE_NAME,
            N::TABLE_NAME,
            node_id
        );

        client.query(&sql).execute().await
    }
}
