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

#[async_trait]
pub trait SimilarityStrategy<N: Node, E: Edge>: Send + Sync {
    async fn find_similar_nodes(
        &self,
        client: &GraphClient<N, E>,
        reference_embedding: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Result<Vec<N>, surrealdb::Error>;
}

pub struct ManualSimilarity;

#[async_trait]
impl<N: Node, E: Edge> SimilarityStrategy<N, E> for ManualSimilarity {
    async fn find_similar_nodes(
        &self,
        client: &GraphClient<N, E>,
        reference_embedding: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Result<Vec<N>, surrealdb::Error> {
        let sql = format!(
            "SELECT *, vector::similarity::cosine(embedding, $reference) as similarity \
             FROM {} WHERE vector::similarity::cosine(embedding, $reference) > $threshold \
             ORDER BY similarity DESC LIMIT $limit",
            N::TABLE_NAME
        );

        client
            .query(&sql)
            .bind("reference", reference_embedding)
            .bind("threshold", threshold)
            .bind("limit", limit)
            .execute()
            .await
    }
}

pub struct NativeVectorSimilarity;

#[async_trait]
impl<N: Node, E: Edge> SimilarityStrategy<N, E> for NativeVectorSimilarity {
    async fn find_similar_nodes(
        &self,
        client: &GraphClient<N, E>,
        reference_embedding: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Result<Vec<N>, surrealdb::Error> {
        let sql = format!(
            "SELECT * FROM {} WHERE vector::similarity::cosine(embedding, $reference) > $threshold LIMIT $limit",
            N::TABLE_NAME
        );

        client
            .query(&sql)
            .bind("reference", reference_embedding.to_vec())
            .bind("threshold", threshold)
            .bind("limit", limit)
            .execute()
            .await
    }
}
