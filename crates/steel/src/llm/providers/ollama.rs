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
pub struct OllamaClient {
    client: Client,
    base_url: String,
    timeout: Duration,
    max_retries: u32,
}

impl OllamaClient {
    pub fn new(
        base_url: Option<String>,
        timeout_seconds: Option<u32>,
        max_retries: Option<u32>,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds.unwrap_or(60).into()))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: base_url.unwrap_or_else(|| "http://localhost:11434".to_string()),
            timeout: Duration::from_secs(timeout_seconds.unwrap_or(60).into()),
            max_retries: max_retries.unwrap_or(3),
        }
    }

    fn build_ollama_payload(&self, request: &ProviderRequest) -> Value {
        let mut payload = json!({
            "model": request.model,
            "messages": request.messages.iter().map(|msg| {
                json!({
                    "role": msg.role,
                    "content": msg.content
                })
            }).collect::<Vec<_>>(),
            "stream": false
        });

        if let Some(max_tokens) = request.max_tokens {
            payload["options"] = json!({
                "num_predict": max_tokens
            });
        }
        if let Some(temperature) = request.temperature {
            if payload["options"].is_null() {
                payload["options"] = json!({});
            }
            payload["options"]["temperature"] = json!(temperature);
        }
        if let Some(top_p) = request.top_p {
            if payload["options"].is_null() {
                payload["options"] = json!({});
            }
            payload["options"]["top_p"] = json!(top_p);
        }
        if let Some(stop) = &request.stop_sequences {
            if payload["options"].is_null() {
                payload["options"] = json!({});
            }
            payload["options"]["stop"] = json!(stop);
        }
        if let Some(stream) = request.stream {
            payload["stream"] = json!(stream);
        }

        for (key, value) in &request.provider_specific {
            payload[key] = value.clone();
        }

        payload
    }

    fn parse_ollama_response(
        &self,
        response_data: Value,
        model: String,
    ) -> LLMResult<ProviderResponse> {
        let content = response_data["message"]["content"]
            .as_str()
            .ok_or_else(|| {
                LLMError::Provider("Failed to extract content from Ollama response".to_string())
            })?;

        let usage = Usage {
            prompt_tokens: response_data["prompt_eval_count"].as_u64().unwrap_or(0) as u32,
            completion_tokens: response_data["eval_count"].as_u64().unwrap_or(0) as u32,
            total_tokens: response_data["prompt_eval_count"].as_u64().unwrap_or(0) as u32
                + response_data["eval_count"].as_u64().unwrap_or(0) as u32,
        };

        let finish_reason = if response_data["done"].as_bool().unwrap_or(false) {
            Some("stop".to_string())
        } else {
            None
        };

        Ok(ProviderResponse {
            content: content.to_string(),
            model,
            usage,
            finish_reason,
            raw_response: response_data,
        })
    }

    async fn execute_request_with_retry(&self, payload: Value, endpoint: &str) -> LLMResult<Value> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            debug!(
                attempt = attempt + 1,
                max_retries = self.max_retries + 1,
                "Sending request to Ollama API"
            );

            let url = format!("{}{}", self.base_url, endpoint);
            let response = tokio::time::timeout(
                self.timeout,
                self.client
                    .post(&url)
                    .header("content-type", "application/json")
                    .json(&payload)
                    .send(),
            )
            .await;

            match response {
                Ok(Ok(resp)) => {
                    let status = resp.status();
                    info!("Received response from Ollama API: {}", status);

                    if status.is_success() {
                        match resp.json::<Value>().await {
                            Ok(data) => {
                                debug!("Successfully parsed Ollama response");
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
                            "Rate limited by Ollama API, waiting {:?} before retry",
                            wait_time
                        );
                        tokio::time::sleep(wait_time).await;
                        last_error = Some(LLMError::RateLimit);
                    } else {
                        let error_body = resp.text().await.unwrap_or_default();
                        last_error = Some(LLMError::Provider(format!(
                            "Ollama API error {status}: {error_body}"
                        )));

                        if status.is_client_error() && status != 429 {
                            break;
                        }
                    }
                }
                Ok(Err(e)) => {
                    last_error = Some(LLMError::Network(format!("Request failed: {e}")));

                    if attempt < self.max_retries {
                        let wait_time = Duration::from_secs(2_u64.pow(attempt.min(3)));
                        tokio::time::sleep(wait_time).await;
                    }
                }
                Err(_) => {
                    warn!("Request to Ollama API timed out after {:?}", self.timeout);
                    last_error = Some(LLMError::Timeout);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| LLMError::Internal("Unknown error".to_string())))
    }
}

#[async_trait]
impl ApiClient for OllamaClient {
    async fn send_request(&self, request: ProviderRequest) -> LLMResult<ProviderResponse> {
        let payload = self.build_ollama_payload(&request);
        let response_data = self
            .execute_request_with_retry(payload, "/api/chat")
            .await?;
        self.parse_ollama_response(response_data, request.model)
    }

    async fn send_streaming_request(
        &self,
        mut request: ProviderRequest,
    ) -> LLMResult<mpsc::UnboundedReceiver<StreamChunk>> {
        request.stream = Some(true);
        let payload = self.build_ollama_payload(&request);

        let (tx, rx) = mpsc::unbounded_channel();
        let client = self.client.clone();
        let base_url = self.base_url.clone();
        let request_id = Uuid::new_v4();

        tokio::spawn(async move {
            let url = format!("{base_url}/api/chat");
            let response = client
                .post(&url)
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

                                            if !line.is_empty() {
                                                if let Ok(parsed) =
                                                    serde_json::from_str::<Value>(&line)
                                                {
                                                    if let Some(content) =
                                                        parsed["message"]["content"].as_str()
                                                    {
                                                        let is_done = parsed["done"]
                                                            .as_bool()
                                                            .unwrap_or(false);

                                                        let _ = tx.send(StreamChunk {
                                                            id: request_id,
                                                            request_id,
                                                            content_delta: content.to_string(),
                                                            is_final: is_done,
                                                            usage: if is_done {
                                                                Some(Usage {
                                                                    prompt_tokens: parsed
                                                                        ["prompt_eval_count"]
                                                                        .as_u64()
                                                                        .unwrap_or(0)
                                                                        as u32,
                                                                    completion_tokens: parsed
                                                                        ["eval_count"]
                                                                        .as_u64()
                                                                        .unwrap_or(0)
                                                                        as u32,
                                                                    total_tokens: parsed
                                                                        ["prompt_eval_count"]
                                                                        .as_u64()
                                                                        .unwrap_or(0)
                                                                        as u32
                                                                        + parsed["eval_count"]
                                                                            .as_u64()
                                                                            .unwrap_or(0)
                                                                            as u32,
                                                                })
                                                            } else {
                                                                None
                                                            },
                                                        });

                                                        if is_done {
                                                            break;
                                                        }
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
        "ollama"
    }

    async fn health_check(&self) -> LLMResult<()> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| LLMError::Network(format!("Failed to connect to Ollama: {e}")))?;

        if response.status().is_success() {
            let models_data: Value = response.json().await.map_err(|e| {
                LLMError::Serialisation(format!("Failed to parse models response: {e}"))
            })?;

            if let Some(models) = models_data["models"].as_array() {
                if models.is_empty() {
                    warn!("Ollama is running but no models are available");
                    return Err(LLMError::Provider(
                        "No models available in Ollama".to_string(),
                    ));
                }

                let model_names: Vec<String> = models
                    .iter()
                    .filter_map(|model| model["name"].as_str().map(|s| s.to_string()))
                    .collect();

                debug!(
                    "Ollama health check successful. Available models: {:?}",
                    model_names
                );
                info!(
                    "Ollama connected with {} models available",
                    model_names.len()
                );
                Ok(())
            } else {
                Err(LLMError::Provider(
                    "Invalid response format from Ollama /api/tags".to_string(),
                ))
            }
        } else {
            Err(LLMError::Provider(format!(
                "Ollama health check failed: {}",
                response.status()
            )))
        }
    }
}
