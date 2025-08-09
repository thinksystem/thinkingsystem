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

use async_trait::async_trait;
use llm_contracts::{LLMRequest, LLMResponse, LLMResult, StreamChunk};

#[async_trait]
pub trait LLMAdapter: Send + Sync {
    async fn generate_response(&self, request: LLMRequest) -> LLMResult<LLMResponse>;

    async fn generate_streaming_response(
        &self,
        request: LLMRequest,
    ) -> LLMResult<tokio::sync::mpsc::Receiver<LLMResult<StreamChunk>>>;

    async fn get_available_models(&self) -> LLMResult<Vec<String>>;

    async fn health_check(&self) -> LLMResult<()>;
}
