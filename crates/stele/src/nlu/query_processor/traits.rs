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

use std::collections::HashMap;
use super::{Result, QueryProcessor};
#[async_trait::async_trait]
pub trait QueryProcessorTrait: Send + Sync {
    async fn process_user_input(&self, input: &str) -> Result<String>;
    async fn health_check(&self) -> Result<HashMap<String, serde_json::Value>>;
    async fn reload_config(&self, config_path: &str) -> Result<()>;
    async fn reload_config_with_retry(&self, config_path: &str, max_retries: u32) -> Result<()>;
    async fn is_reload_safe(&self) -> Result<bool>;
    async fn get_config_summary(&self) -> Result<HashMap<String, serde_json::Value>>;
}
#[async_trait::async_trait]
impl QueryProcessorTrait for QueryProcessor {
    async fn process_user_input(&self, input: &str) -> Result<String> {
        QueryProcessor::process_user_input(self, input).await
    }
    async fn health_check(&self) -> Result<HashMap<String, serde_json::Value>> {
        self.perform_health_check().await
    }
    async fn reload_config(&self, config_path: &str) -> Result<()> {
        QueryProcessor::reload_config(self, config_path).await
    }
    async fn reload_config_with_retry(&self, config_path: &str, max_retries: u32) -> Result<()> {
        QueryProcessor::reload_config_with_retry(self, config_path, max_retries).await
    }
    async fn is_reload_safe(&self) -> Result<bool> {
        QueryProcessor::is_reload_safe(self).await
    }
    async fn get_config_summary(&self) -> Result<HashMap<String, serde_json::Value>> {
        QueryProcessor::get_config_summary(self).await
    }
}
