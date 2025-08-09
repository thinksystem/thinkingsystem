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

use crate::llm::{LLMError, LLMResult};
use futures::{future, Stream};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use stele::nlu::llm_processor::LLMAdapter;
use stele::LLMConfig;
use tracing::warn;

#[async_trait::async_trait]
pub trait StreamingLLMAdapter {
    async fn stream_response(
        &self,
        prompt: &str,
    ) -> Result<Box<dyn Stream<Item = Result<String, LLMError>> + Unpin + Send>, LLMError>;

    fn get_config(&self) -> &LLMConfig;
}

pub struct StreamingWrapper<T: LLMAdapter> {
    inner: T,
    config: LLMConfig,
}

impl<T: LLMAdapter> StreamingWrapper<T> {
    pub fn new(adapter: T, config: LLMConfig) -> Self {
        Self {
            inner: adapter,
            config,
        }
    }
}

#[async_trait::async_trait]
impl<T: LLMAdapter + Send + Sync> LLMAdapter for StreamingWrapper<T> {
    async fn process_text(&self, input: &str) -> Result<String, Box<dyn std::error::Error>> {
        self.inner.process_text(input).await
    }

    async fn generate_response(&self, prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
        self.inner.generate_response(prompt).await
    }

    async fn generate_structured_response(
        &self,
        system_prompt: &str,
        user_input: &str,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        self.inner
            .generate_structured_response(system_prompt, user_input)
            .await
    }
}

#[async_trait::async_trait]
impl<T: LLMAdapter + Send + Sync> StreamingLLMAdapter for StreamingWrapper<T> {
    async fn stream_response(
        &self,
        prompt: &str,
    ) -> Result<Box<dyn Stream<Item = Result<String, LLMError>> + Unpin + Send>, LLMError> {
        let response = self
            .inner
            .generate_response(prompt)
            .await
            .map_err(|e| LLMError::ApiError(e.to_string()))?;

        let words: Vec<String> = response
            .split_whitespace()
            .map(|word| format!("{word} "))
            .collect();

        let stream = futures::stream::iter(words.into_iter().map(Ok));
        Ok(Box::new(stream))
    }

    fn get_config(&self) -> &LLMConfig {
        &self.config
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationEntry {
    pub role: String,
    pub content: String,
    pub timestamp: u64,
    pub metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationConfig {
    pub max_history_length: usize,
    pub context_window_tokens: usize,
    pub preserve_system_messages: bool,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            max_history_length: 50,
            context_window_tokens: 4000,
            preserve_system_messages: true,
        }
    }
}

pub struct LLMProcessor {
    adapter: Box<dyn LLMAdapter>,
    streaming_config: LLMConfig,
    config: ConversationConfig,

    conversation_history: VecDeque<ConversationEntry>,
}

impl LLMProcessor {
    pub fn new(adapter: Box<dyn LLMAdapter>, config: ConversationConfig) -> Self {
        let llm_config = LLMConfig::default();
        Self {
            adapter,
            streaming_config: llm_config,
            config,
            conversation_history: VecDeque::new(),
        }
    }

    pub fn add_message(&mut self, role: &str, content: &str) {
        let entry = ConversationEntry {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            metadata: HashMap::new(),
        };
        self.conversation_history.push_back(entry);

        while self.conversation_history.len() > self.config.max_history_length {
            self.conversation_history.pop_front();
        }
    }

    pub async fn process_message(&mut self, user_message: &str) -> LLMResult<String> {
        self.add_message("user", user_message);
        let context = self.build_context(None);
        let response = self
            .adapter
            .generate_response(&context)
            .await
            .map_err(|e| LLMError::ApiError(e.to_string()))?;
        self.add_message("assistant", &response);
        Ok(response)
    }

    pub async fn process_with_context(
        &mut self,
        system_prompt: &str,
        user_message: &str,
    ) -> LLMResult<Value> {
        self.add_message("user", user_message);
        let response = self
            .adapter
            .generate_structured_response(system_prompt, user_message)
            .await
            .map_err(|e| LLMError::ApiError(e.to_string()))?;
        let response_text = response
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("Generated structured response");
        self.add_message("assistant", response_text);
        Ok(response)
    }

    pub async fn stream_process_message(
        &mut self,
        user_message: &str,
    ) -> LLMResult<Box<dyn Stream<Item = LLMResult<String>> + Unpin + Send>> {
        self.add_message("user", user_message);

        let context = self.build_context(None);

        let response = self
            .adapter
            .generate_response(&context)
            .await
            .map_err(|e| LLMError::ApiError(e.to_string()))?;

        let words: Vec<LLMResult<String>> = response
            .split_whitespace()
            .map(|word| Ok(format!("{word} ")))
            .collect();

        self.add_message("assistant", &response);

        let result_stream = futures::stream::iter(words);
        Ok(Box::new(result_stream))
    }

    pub async fn stream_process_with_callback<F>(
        &mut self,
        user_message: &str,
        mut callback: F,
    ) -> LLMResult<String>
    where
        F: FnMut(&str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>,
    {
        self.add_message("user", user_message);
        let context = self.build_context(None);

        let response = self
            .adapter
            .generate_response(&context)
            .await
            .map_err(|e| LLMError::ApiError(e.to_string()))?;

        for word in response.split_whitespace() {
            let chunk = format!("{word} ");
            if let Err(e) = callback(&chunk) {
                warn!("Callback error during streaming: {}", e);
                return Err(LLMError::StreamError(format!("Callback failed: {e}")));
            }
        }

        self.add_message("assistant", &response);
        Ok(response)
    }

    pub async fn stream_process_with_delay(
        &mut self,
        user_message: &str,
        _delay_ms: u64,
    ) -> LLMResult<Box<dyn Stream<Item = LLMResult<String>> + Unpin + Send>> {
        self.add_message("user", user_message);
        let context = self.build_context(None);

        let response = self
            .adapter
            .generate_response(&context)
            .await
            .map_err(|e| LLMError::ApiError(e.to_string()))?;

        let words: Vec<LLMResult<String>> = response
            .split_whitespace()
            .map(|word| Ok(format!("{word} ")))
            .collect();

        self.add_message("assistant", &response);

        let result_stream = futures::stream::iter(words);
        Ok(Box::new(result_stream))
    }

    pub fn get_history(&self) -> &VecDeque<ConversationEntry> {
        &self.conversation_history
    }

    pub fn clear_history(&mut self) {
        self.conversation_history.clear();
    }

    pub fn get_adapter_config(&self) -> &LLMConfig {
        &self.streaming_config
    }

    pub fn build_context(&self, max_entries: Option<usize>) -> String {
        let limit = max_entries.unwrap_or(self.conversation_history.len());
        self.conversation_history
            .iter()
            .rev()
            .take(limit)
            .rev()
            .map(|entry| format!("{}: {}", entry.role.to_uppercase(), entry.content))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub async fn generate_chat_response(
    adapter: &dyn LLMAdapter,
    user_input: &str,
) -> LLMResult<String> {
    let prompt = format!("User: {user_input}\nAssistant:");
    adapter
        .generate_response(&prompt)
        .await
        .map_err(|e| LLMError::ApiError(e.to_string()))
}

pub async fn extract_structured_data(
    adapter: &dyn LLMAdapter,
    prompt: &str,
    schema_description: &str,
) -> LLMResult<Value> {
    let system_prompt = format!(
        "You are a data extraction assistant. Extract information according to this schema: {schema_description}\n\
        Return the result as valid JSON."
    );
    adapter
        .generate_structured_response(&system_prompt, prompt)
        .await
        .map_err(|e| LLMError::ApiError(e.to_string()))
}

pub async fn generate_best_response(
    adapter: &dyn LLMAdapter,
    prompt: &str,
    num_candidates: usize,
) -> LLMResult<String> {
    let futures = (0..num_candidates).map(|_| adapter.generate_response(prompt));
    let results = future::join_all(futures).await;

    let successful_responses: Vec<String> = results.into_iter().filter_map(Result::ok).collect();

    if successful_responses.is_empty() {
        return Err(LLMError::ApiError(
            "No successful responses were generated".to_string(),
        ));
    }

    successful_responses
        .into_iter()
        .max_by_key(|r| r.len())
        .ok_or_else(|| LLMError::ApiError("Failed to select best response".to_string()))
}

pub async fn batch_process(adapter: &dyn LLMAdapter, prompts: &[String]) -> Vec<LLMResult<String>> {
    let futures = prompts.iter().map(|p| adapter.generate_response(p));
    let results = future::join_all(futures).await;
    results
        .into_iter()
        .map(|r| r.map_err(|e| LLMError::ApiError(e.to_string())))
        .collect()
}

pub async fn stream_generate_response(
    adapter: &dyn LLMAdapter,
    prompt: &str,
) -> LLMResult<Box<dyn Stream<Item = LLMResult<String>> + Unpin + Send>> {
    let response = adapter
        .generate_response(prompt)
        .await
        .map_err(|e| LLMError::ApiError(e.to_string()))?;

    let words: Vec<LLMResult<String>> = response
        .split_whitespace()
        .map(|word| Ok(format!("{word} ")))
        .collect();

    let stream = futures::stream::iter(words);
    Ok(Box::new(stream))
}

pub async fn stream_with_callback<F>(
    adapter: &dyn LLMAdapter,
    prompt: &str,
    mut callback: F,
) -> LLMResult<String>
where
    F: FnMut(&str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>,
{
    let response = adapter
        .generate_response(prompt)
        .await
        .map_err(|e| LLMError::ApiError(e.to_string()))?;

    for word in response.split_whitespace() {
        let chunk = format!("{word} ");
        if let Err(e) = callback(&chunk) {
            return Err(LLMError::StreamError(format!("Callback failed: {e}")));
        }
    }

    Ok(response)
}
