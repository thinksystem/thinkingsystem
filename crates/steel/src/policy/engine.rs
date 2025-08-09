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

use crate::policy::ast::{evaluate, EvaluationContext, Expression};
use crate::policy::parser::ConditionParser;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

#[derive(Debug, Deserialize, Clone)]
pub struct Policy {
    pub name: String,
    pub role: String,
    pub action: String,
    pub resource: String,
    #[serde(default = "default_effect")]
    pub effect: String,
    pub conditions: Vec<String>,
}

fn default_effect() -> String {
    "allow".to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuthorisationDecision {
    Allow,
    Deny(String),
}

pub struct PolicyEngine {
    policies: Vec<ParsedPolicy>,
}

#[derive(Debug, Clone)]
struct ParsedPolicy {
    name: String,
    role: String,
    action: String,
    resource: String,
    effect: String,
    parsed_conditions: Vec<Expression>,
}

impl PolicyEngine {
    pub fn new(policies: Vec<Policy>) -> Result<Self, String> {
        let mut parsed_policies = Vec::new();

        for policy in policies {
            let mut parsed_conditions = Vec::new();

            for condition_str in &policy.conditions {
                match ConditionParser::parse(condition_str) {
                    Ok(expr) => parsed_conditions.push(expr),
                    Err(e) => {
                        warn!(
                            "Failed to parse condition '{}' in policy '{}': {}",
                            condition_str, policy.name, e
                        );
                        return Err(format!(
                            "Policy parsing failed for '{}': {}",
                            policy.name, e
                        ));
                    }
                }
            }

            parsed_policies.push(ParsedPolicy {
                name: policy.name,
                role: policy.role,
                action: policy.action,
                resource: policy.resource,
                effect: policy.effect,
                parsed_conditions,
            });
        }

        info!(
            "✅ Policy engine initialised with {} parsed policies",
            parsed_policies.len()
        );
        Ok(Self {
            policies: parsed_policies,
        })
    }

    pub fn authorise(
        &self,
        user_roles: &[String],
        action: &str,
        resource: &str,
        data: &Value,
    ) -> AuthorisationDecision {
        debug!(
            "🔍 Authorisation check: roles={:?}, action={}, resource={}",
            user_roles, action, resource
        );

        let context = EvaluationContext::new(data.clone());
        let mut allow_found = false;

        for policy in &self.policies {
            if !self.policy_matches(policy, action, resource) {
                continue;
            }

            if !self.role_matches(policy, user_roles) {
                continue;
            }

            if policy.effect == "deny" && self.evaluate_conditions(policy, &context) {
                let reason = format!(
                    "🚫 Denied by policy '{}': User with roles {:?} is explicitly blocked",
                    policy.name, user_roles
                );
                error!("{}", reason);
                return AuthorisationDecision::Deny(reason);
            }
        }

        for policy in &self.policies {
            if !self.policy_matches(policy, action, resource) {
                continue;
            }

            if !self.role_matches(policy, user_roles) {
                continue;
            }

            if policy.effect == "allow" && self.evaluate_conditions(policy, &context) {
                info!("✅ Allowed by policy '{}'", policy.name);
                allow_found = true;
                break;
            }
        }

        if allow_found {
            AuthorisationDecision::Allow
        } else {
            let reason = format!(
                "🚫 Implicitly denied: No matching 'allow' policy found for user with roles {user_roles:?}"
            );
            warn!("{}", reason);
            AuthorisationDecision::Deny(reason)
        }
    }

    fn policy_matches(&self, policy: &ParsedPolicy, action: &str, resource: &str) -> bool {
        policy.action == action && policy.resource == resource
    }

    fn role_matches(&self, policy: &ParsedPolicy, user_roles: &[String]) -> bool {
        policy.role == "any" || user_roles.contains(&policy.role)
    }

    fn evaluate_conditions(&self, policy: &ParsedPolicy, context: &EvaluationContext) -> bool {
        if policy.parsed_conditions.is_empty() {
            return true;
        }

        for condition in &policy.parsed_conditions {
            let result = evaluate(condition, context);
            if !result.is_true() {
                debug!(
                    "❌ Condition failed in policy '{}': {}",
                    policy.name, condition
                );
                return false;
            }
        }

        debug!("✅ All conditions passed for policy '{}'", policy.name);
        true
    }

    pub fn get_policy_summary(&self) -> HashMap<String, serde_json::Value> {
        let mut summary = HashMap::new();

        for policy in &self.policies {
            summary.insert(
                policy.name.clone(),
                serde_json::json!({
                    "role": policy.role,
                    "action": policy.action,
                    "resource": policy.resource,
                    "effect": policy.effect,
                    "conditions_count": policy.parsed_conditions.len()
                }),
            );
        }

        summary
    }
}
