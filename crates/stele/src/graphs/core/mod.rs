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

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;
use surrealdb::sql::{Thing, Value as SurrealValue};
use surrealdb::{engine::remote::ws::Client, Surreal};

use crate::graphs::strategies::{
    FetchNeighbours, ManualNeighbours, ManualPathfinder, ManualSimilarity, NativeGraphRoute,
    NativeVectorSimilarity, NeighbourStrategy, PathfindingStrategy, SimilarityStrategy,
};

pub trait Record: Serialize + DeserializeOwned + Send + Sync + 'static {
    const TABLE_NAME: &'static str;
}

pub trait Node: Record {
    fn get_id(&self) -> &Thing;
}

pub trait Edge: Record {}

#[derive(Debug, Clone, Default)]
pub struct GraphClientConfig {
    pub use_native_pathfinding: bool,
    pub use_fetch_neighbours: bool,
    pub use_native_similarity: bool,
}

impl GraphClientConfig {
    pub fn native() -> Self {
        Self {
            use_native_pathfinding: true,
            use_fetch_neighbours: true,
            use_native_similarity: true,
        }
    }

    pub fn robust() -> Self {
        Self::default()
    }
}

#[derive(Clone)]
pub struct GraphClient<N: Node, E: Edge> {
    executor: DatabaseExecutor,
    pathfinder: Arc<dyn PathfindingStrategy<N, E>>,
    neighbour_finder: Arc<dyn NeighbourStrategy<N, E>>,
    similarity_finder: Arc<dyn SimilarityStrategy<N, E>>,
    _node: PhantomData<N>,
    _edge: PhantomData<E>,
}

impl<N, E> GraphClient<N, E>
where
    N: Node,
    E: Edge,
{
    pub fn new(db: Arc<Surreal<Client>>) -> Self {
        Self::with_config(db, GraphClientConfig::default())
    }

    pub fn with_config(db: Arc<Surreal<Client>>, config: GraphClientConfig) -> Self {
        let pathfinder: Arc<dyn PathfindingStrategy<N, E>> = if config.use_native_pathfinding {
            Arc::new(NativeGraphRoute)
        } else {
            Arc::new(ManualPathfinder)
        };

        let neighbour_finder: Arc<dyn NeighbourStrategy<N, E>> = if config.use_fetch_neighbours {
            Arc::new(FetchNeighbours)
        } else {
            Arc::new(ManualNeighbours)
        };

        let similarity_finder: Arc<dyn SimilarityStrategy<N, E>> = if config.use_native_similarity {
            Arc::new(NativeVectorSimilarity)
        } else {
            Arc::new(ManualSimilarity)
        };

        Self {
            executor: DatabaseExecutor::new(db),
            pathfinder,
            neighbour_finder,
            similarity_finder,
            _node: PhantomData,
            _edge: PhantomData,
        }
    }

    pub async fn define(&self, schema_query: &str) -> Result<(), surrealdb::Error> {
        self.executor.client.query(schema_query).await?;
        Ok(())
    }

    pub async fn add_node(&self, node: N) -> Result<N, surrealdb::Error> {
        self.executor.create(N::TABLE_NAME, node).await
    }

    pub async fn add_edge(
        &self,
        from: &Thing,
        to: &Thing,
        edge: E,
    ) -> Result<Option<E>, surrealdb::Error> {
        let edge_content = serde_json::to_value(&edge).map_err(|e| {
            surrealdb::Error::Api(surrealdb::error::Api::InvalidParams(format!(
                "Failed to serialise edge: {e}"
            )))
        })?;
        let mut content_map = edge_content.as_object().unwrap().clone();
        content_map.remove("id");

        let sql = format!("RELATE $from->{}->$to CONTENT $content", E::TABLE_NAME);

        let mut result = self
            .executor
            .client
            .query(&sql)
            .bind(("from", from.clone()))
            .bind(("to", to.clone()))
            .bind(("content", content_map))
            .await?;

        let created_relations: Vec<E> = result.take(0)?;
        Ok(created_relations.into_iter().next())
    }

    pub async fn get_node(&self, id: &Thing) -> Result<Option<N>, surrealdb::Error> {
        self.executor.select(id).await
    }

    pub async fn get_neighbours(&self, node_id: &Thing) -> Result<Vec<N>, surrealdb::Error> {
        self.neighbour_finder.get_neighbours(self, node_id).await
    }

    pub async fn route_path(
        &self,
        from: &Thing,
        to: &Thing,
    ) -> Result<Vec<SurrealValue>, surrealdb::Error> {
        self.pathfinder.find_path(self, from, to).await
    }

    pub async fn find_nodes_by_similarity(
        &self,
        reference_embedding: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Result<Vec<N>, surrealdb::Error> {
        self.similarity_finder
            .find_similar_nodes(self, reference_embedding, threshold, limit)
            .await
    }

    pub async fn count_edges(&self) -> Result<i64, surrealdb::Error> {
        let count_sql = format!("SELECT count() FROM {}", E::TABLE_NAME);
        let mut result = self.executor.client.query(&count_sql).await?;
        let counts: Vec<serde_json::Value> = result.take(0)?;

        if let Some(first) = counts.first() {
            if let Some(count) = first.get("count") {
                if let Some(num) = count.as_i64() {
                    return Ok(num);
                }
            }
        }
        Ok(0)
    }

    pub fn query<'a>(&self, query: &'a str) -> QueryBuilder<'a> {
        QueryBuilder::new(self.executor.clone(), query)
    }

    pub async fn health_check(&self) -> Result<bool, surrealdb::Error> {
        let client = self.client();
        let mut result = client.query("SELECT 1 as health").await?;
        let _: Vec<serde_json::Value> = result.take(0)?;
        Ok(true)
    }

    pub(crate) fn client(&self) -> &Arc<Surreal<Client>> {
        &self.executor.client
    }
}

#[derive(Clone)]
pub struct DatabaseExecutor {
    client: Arc<Surreal<Client>>,
}

impl DatabaseExecutor {
    pub fn new(client: Arc<Surreal<Client>>) -> Self {
        Self { client }
    }

    pub async fn create<T: Record>(&self, table: &str, data: T) -> Result<T, surrealdb::Error> {
        let record: Option<T> = self.client.create(table).content(data).await?;
        record.ok_or_else(|| {
            surrealdb::Error::Api(surrealdb::error::Api::InvalidParams(
                "Create operation returned no result".into(),
            ))
        })
    }

    pub async fn select<T: Record>(&self, thing: &Thing) -> Result<Option<T>, surrealdb::Error> {
        self.client
            .select((thing.tb.as_str(), thing.id.to_string()))
            .await
    }
}

pub struct QueryBuilder<'a> {
    executor: DatabaseExecutor,
    query: &'a str,
    bindings: HashMap<String, Value>,
}

impl<'a> QueryBuilder<'a> {
    pub fn new(executor: DatabaseExecutor, query: &'a str) -> Self {
        Self {
            executor,
            query,
            bindings: HashMap::new(),
        }
    }

    pub fn bind<T: Serialize>(mut self, key: &str, value: T) -> Self {
        if let Ok(val) = serde_json::to_value(value) {
            self.bindings.insert(key.to_string(), val);
        }
        self
    }

    pub async fn execute<T: DeserializeOwned>(self) -> Result<Vec<T>, surrealdb::Error> {
        let mut query = self.executor.client.query(self.query);
        for (key, value) in self.bindings {
            query = query.bind((key, value));
        }
        query.await?.take(0)
    }
}
