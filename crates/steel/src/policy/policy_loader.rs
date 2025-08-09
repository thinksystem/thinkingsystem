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

use crate::policy::engine::{Policy, PolicyEngine};
use serde::Deserialize;
use std::collections::HashMap;
use tokio::fs;
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
pub struct PolicyConfig {
    pub providers: Vec<ProviderConfig>,
    pub policies: Vec<Policy>,
}

#[derive(Debug, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub connection_type: String,
    pub config: HashMap<String, String>,
}

pub struct PolicyLoader;

impl PolicyLoader {
    pub async fn load_from_file(
        file_path: &str,
    ) -> Result<PolicyEngine, Box<dyn std::error::Error>> {
        info!("Loading policy configuration from {}", file_path);

        let yaml_content = fs::read_to_string(file_path).await?;
        let policy_config: PolicyConfig = serde_yaml::from_str(&yaml_content)?;

        info!(
            " Loaded {} providers and {} policies from configuration",
            policy_config.providers.len(),
            policy_config.policies.len()
        );

        Self::validate_policies(&policy_config.policies)?;

        let engine = PolicyEngine::new(policy_config.policies)?;

        Ok(engine)
    }

    pub async fn reload_from_file(
        file_path: &str,
    ) -> Result<PolicyEngine, Box<dyn std::error::Error>> {
        info!("Reloading policy configuration from {}", file_path);
        Self::load_from_file(file_path).await
    }

    fn validate_policies(policies: &[Policy]) -> Result<(), String> {
        let mut role_counts = HashMap::new();
        let mut resource_actions = std::collections::HashSet::new();

        for policy in policies {
            *role_counts.entry(policy.role.clone()).or_insert(0) += 1;

            let key = format!("{}:{}", policy.resource, policy.action);
            resource_actions.insert(key);

            if policy.effect != "allow" && policy.effect != "deny" {
                return Err(format!(
                    "Invalid effect '{}' in policy '{}'. Must be 'allow' or 'deny'.",
                    policy.effect, policy.name
                ));
            }

            if policy.conditions.is_empty() {
                warn!(
                    "️⚠️ Policy '{}' has no conditions - it will always apply when role/action/resource match",
                    policy.name
                );
            }
        }

        info!("Policy distribution by role:");
        for (role, count) in &role_counts {
            info!("- {}: {} policies", role, count);
        }

        info!(
            " Resource-action combinations covered: {}",
            resource_actions.len()
        );

        Ok(())
    }

    pub async fn load_providers_from_file(
        file_path: &str,
    ) -> Result<Vec<ProviderConfig>, Box<dyn std::error::Error>> {
        let yaml_content = fs::read_to_string(file_path).await?;
        let policy_config: PolicyConfig = serde_yaml::from_str(&yaml_content)?;
        Ok(policy_config.providers)
    }
}
