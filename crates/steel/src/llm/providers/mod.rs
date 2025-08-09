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

pub mod anthropic;
pub mod ollama;
pub mod openai;

use async_trait::async_trait;
use llm_contracts::{LLMResult, ProviderRequest, ProviderResponse, StreamChunk};
use tokio::sync::mpsc;

#[async_trait]
pub trait ApiClient: Send + Sync {
    async fn send_request(&self, request: ProviderRequest) -> LLMResult<ProviderResponse>;

    async fn send_streaming_request(
        &self,
        request: ProviderRequest,
    ) -> LLMResult<mpsc::UnboundedReceiver<StreamChunk>>;

    fn provider_name(&self) -> &'static str;

    async fn health_check(&self) -> LLMResult<()>;
}

pub use anthropic::AnthropicClient;
pub use ollama::OllamaClient;
pub use openai::OpenAIClient;
