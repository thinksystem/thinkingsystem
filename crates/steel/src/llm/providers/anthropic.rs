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
use llm_contracts::{LLMError, LLMResult, ProviderRequest, ProviderResponse, StreamChunk, Usage};
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::ApiClient;

#[derive(Debug, Clone)]
pub struct AnthropicClient {
    client: Client,
    api_key: String,
    endpoint: String,
    api_version: String,
    timeout: Duration,
    max_retries: u32,
}

impl AnthropicClient {
    pub fn new(
        api_key: String,
        endpoint: Option<String>,
        api_version: Option<String>,
        timeout_seconds: Option<u32>,
        max_retries: Option<u32>,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds.unwrap_or(30).into()))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            api_key,
            endpoint: endpoint
                .unwrap_or_else(|| "https://api.anthropic.com/v1/messages".to_string()),
            api_version: api_version.unwrap_or_else(|| "2023-06-01".to_string()),
            timeout: Duration::from_secs(timeout_seconds.unwrap_or(30).into()),
            max_retries: max_retries.unwrap_or(3),
        }
    }

    fn build_anthropic_payload(&self, request: &ProviderRequest) -> Value {
        let mut system_content = Vec::new();
        let mut regular_messages = Vec::new();

        for msg in &request.messages {
            if msg.role == "system" {
                system_content.push(msg.content.clone());
            } else {
                regular_messages.push(json!({
                    "role": msg.role,
                    "content": msg.content
                }));
            }
        }

        let mut payload = json!({
            "model": request.model,
            "messages": regular_messages
        });

        if !system_content.is_empty() {
            payload["system"] = json!(system_content.join("\n\n"));
        }

        if let Some(max_tokens) = request.max_tokens {
            payload["max_tokens"] = json!(max_tokens);
        } else {
            payload["max_tokens"] = json!(4096);
        }
        if let Some(temperature) = request.temperature {
            payload["temperature"] = json!(temperature);
        }
        if let Some(top_p) = request.top_p {
            payload["top_p"] = json!(top_p);
        }
        if let Some(stop) = &request.stop_sequences {
            payload["stop_sequences"] = json!(stop);
        }
        if let Some(stream) = request.stream {
            payload["stream"] = json!(stream);
        }

        for (key, value) in &request.provider_specific {
            payload[key] = value.clone();
        }

        payload
    }

    fn parse_anthropic_response(
        &self,
        response_data: Value,
        model: String,
    ) -> LLMResult<ProviderResponse> {
        let content = response_data["content"][0]["text"]
            .as_str()
            .ok_or_else(|| {
                LLMError::Provider("Failed to extract content from Anthropic response".to_string())
            })?;

        let usage = if let Some(usage_data) = response_data.get("usage") {
            Usage {
                prompt_tokens: usage_data["input_tokens"].as_u64().unwrap_or(0) as u32,
                completion_tokens: usage_data["output_tokens"].as_u64().unwrap_or(0) as u32,
                total_tokens: usage_data["input_tokens"].as_u64().unwrap_or(0) as u32
                    + usage_data["output_tokens"].as_u64().unwrap_or(0) as u32,
            }
        } else {
            Usage::default()
        };

        let finish_reason = response_data["stop_reason"].as_str().map(|s| s.to_string());

        Ok(ProviderResponse {
            content: content.to_string(),
            model,
            usage,
            finish_reason,
            raw_response: response_data,
        })
    }

    async fn execute_request_with_retry(&self, payload: Value) -> LLMResult<Value> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            debug!(
                attempt = attempt + 1,
                max_retries = self.max_retries + 1,
                "Sending request to Anthropic API"
            );

            let response = tokio::time::timeout(
                self.timeout,
                self.client
                    .post(&self.endpoint)
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", &self.api_version)
                    .header("content-type", "application/json")
                    .json(&payload)
                    .send(),
            )
            .await;

            match response {
                Ok(Ok(resp)) => {
                    let status = resp.status();
                    info!("Received response from Anthropic API: {}", status);

                    if status.is_success() {
                        match resp.json::<Value>().await {
                            Ok(data) => {
                                debug!("Successfully parsed Anthropic response");
                                return Ok(data);
                            }
                            Err(e) => {
                                last_error = Some(LLMError::Serialisation(format!(
                                    "Failed to parse JSON response: {e}"
                                )));
                            }
                        }
                    } else if status == 429 {
                        let wait_time = Duration::from_secs(2_u64.pow(attempt.min(5)));
                        warn!(
                            "Rate limited by Anthropic API, waiting {:?} before retry",
                            wait_time
                        );
                        tokio::time::sleep(wait_time).await;
                        last_error = Some(LLMError::RateLimit);
                    } else {
                        match resp.text().await {
                            Ok(error_body) => {
                                last_error = Some(LLMError::Provider(format!(
                                    "Anthropic API error {status}: {error_body}"
                                )));
                            }
                            Err(e) => {
                                last_error = Some(LLMError::Provider(format!(
                                    "Anthropic API error {status}, failed to read error body: {e}"
                                )));
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    last_error = Some(LLMError::Network(format!("Request failed: {e}")));
                }
                Err(_) => {
                    warn!(
                        "Request to Anthropic API timed out after {} seconds",
                        self.timeout.as_secs()
                    );
                    last_error = Some(LLMError::Timeout);
                }
            }

            if attempt < self.max_retries {
                let wait_time = Duration::from_secs(2_u64.pow(attempt.min(3)));
                tokio::time::sleep(wait_time).await;
            }
        }

        Err(last_error.unwrap_or_else(|| LLMError::Internal("Unknown error".to_string())))
    }
}

#[async_trait]
impl ApiClient for AnthropicClient {
    async fn send_request(&self, request: ProviderRequest) -> LLMResult<ProviderResponse> {
        let payload = self.build_anthropic_payload(&request);
        let response_data = self.execute_request_with_retry(payload).await?;
        self.parse_anthropic_response(response_data, request.model)
    }

    async fn send_streaming_request(
        &self,
        mut request: ProviderRequest,
    ) -> LLMResult<mpsc::UnboundedReceiver<StreamChunk>> {
        request.stream = Some(true);
        let payload = self.build_anthropic_payload(&request);

        let (tx, rx) = mpsc::unbounded_channel();
        let client = self.client.clone();
        let endpoint = self.endpoint.clone();
        let api_key = self.api_key.clone();
        let api_version = self.api_version.clone();
        let request_id = Uuid::new_v4();

        tokio::spawn(async move {
            let response = client
                .post(&endpoint)
                .header("x-api-key", &api_key)
                .header("anthropic-version", &api_version)
                .header("content-type", "application/json")
                .json(&payload)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let mut stream = resp.bytes_stream();
                        let mut buffer = String::new();

                        while let Some(chunk_result) = stream.next().await {
                            match chunk_result {
                                Ok(chunk) => {
                                    if let Ok(text) = String::from_utf8(chunk.to_vec()) {
                                        buffer.push_str(&text);

                                        while let Some(line_end) = buffer.find('\n') {
                                            let line = buffer[..line_end].trim().to_string();
                                            buffer = buffer.split_off(line_end + 1);

                                            if let Some(data) = line.strip_prefix("data: ") {
                                                if data == "[DONE]" {
                                                    let _ = tx.send(StreamChunk {
                                                        id: request_id,
                                                        request_id,
                                                        content_delta: "".to_string(),
                                                        is_final: true,
                                                        usage: None,
                                                    });
                                                    break;
                                                }

                                                if let Ok(parsed) =
                                                    serde_json::from_str::<Value>(data)
                                                {
                                                    if let Some(delta) = parsed
                                                        .get("delta")
                                                        .and_then(|d| d.get("text"))
                                                        .and_then(|t| t.as_str())
                                                    {
                                                        let _ = tx.send(StreamChunk {
                                                            id: request_id,
                                                            request_id,
                                                            content_delta: delta.to_string(),
                                                            is_final: false,
                                                            usage: None,
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    }
                }
                Err(_) => {
                    let _ = tx.send(StreamChunk {
                        id: request_id,
                        request_id,
                        content_delta: "".to_string(),
                        is_final: true,
                        usage: None,
                    });
                }
            }
        });

        Ok(rx)
    }

    fn provider_name(&self) -> &'static str {
        "anthropic"
    }

    async fn health_check(&self) -> LLMResult<()> {
        let test_request = ProviderRequest {
            model: "claude-3-5-haiku-latest".to_string(),
            messages: vec![llm_contracts::Message {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            max_tokens: Some(10),
            temperature: Some(0.1),
            top_p: None,
            stop_sequences: None,
            stream: Some(false),
            provider_specific: std::collections::HashMap::new(),
        };

        self.send_request(test_request).await?;
        Ok(())
    }
}
