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



pub mod anomaly;
pub mod backpressure;

use async_trait::async_trait;


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptanceDecision {
    Accept { rationale: String },
    Reject { rationale: String },
    Defer { rationale: String },
}


#[derive(Debug, Clone)]
pub struct PolicyInput {
    pub kind: String,               
    pub payload: serde_json::Value, 
    pub context: serde_json::Value, 
}

#[async_trait]
pub trait PolicyModule: Send + Sync {
    fn name(&self) -> &str;
    async fn evaluate(&self, input: &PolicyInput) -> AcceptanceDecision;
}


pub struct WasmPolicyModule {
    pub name: String,
    pub wasm_bytes: Vec<u8>,
}

#[async_trait]
impl PolicyModule for WasmPolicyModule {
    fn name(&self) -> &str {
        &self.name
    }
    async fn evaluate(&self, input: &PolicyInput) -> AcceptanceDecision {
        
        let _ = input;
        AcceptanceDecision::Defer {
            rationale: "wasm-not-implemented".into(),
        }
    }
}


pub struct DslPolicyModule {
    pub name: String,
    pub rule_src: String,
}

#[async_trait]
impl PolicyModule for DslPolicyModule {
    fn name(&self) -> &str {
        &self.name
    }
    async fn evaluate(&self, input: &PolicyInput) -> AcceptanceDecision {
        
        let rule = &self.rule_src;
        if let Some(th_pos) = rule.find("CONF<") {
            if let Ok(th) = rule[th_pos + 5..].trim().parse::<f64>() {
                if let Some(c) = input.payload.get("confidence").and_then(|v| v.as_f64()) {
                    if c < th {
                        return AcceptanceDecision::Reject {
                            rationale: format!("confidence {c} below threshold {th}"),
                        };
                    }
                }
            }
        }
        AcceptanceDecision::Accept {
            rationale: "rule-pass".into(),
        }
    }
}


pub struct ConsensusPolicyEngine {
    modules: Vec<std::sync::Arc<dyn PolicyModule>>,
    pub quorum: usize,
}

impl ConsensusPolicyEngine {
    pub fn new(mods: Vec<std::sync::Arc<dyn PolicyModule>>, quorum: usize) -> Self {
        Self {
            modules: mods,
            quorum,
        }
    }
    pub async fn evaluate(&self, input: &PolicyInput) -> AcceptanceDecision {
        let mut accept = 0usize;
        let mut reject = 0usize;
        let mut _defer = 0usize;
        let mut rationales = Vec::new();
        for m in &self.modules {
            let decision = m.evaluate(input).await;
            rationales.push(format!("{}:{:?}", m.name(), decision));
            match decision {
                AcceptanceDecision::Accept { .. } => accept += 1,
                AcceptanceDecision::Reject { .. } => reject += 1,
                AcceptanceDecision::Defer { .. } => _defer += 1,
            }
            if accept >= self.quorum {
                return AcceptanceDecision::Accept {
                    rationale: format!("quorum accept: {}", rationales.join(" | ")),
                };
            }
            if reject >= self.quorum {
                return AcceptanceDecision::Reject {
                    rationale: format!("quorum reject: {}", rationales.join(" | ")),
                };
            }
        }
        
        AcceptanceDecision::Defer {
            rationale: format!("no quorum: {}", rationales.join(" | ")),
        }
    }
}




pub fn consensus_from_env() -> Option<ConsensusPolicyEngine> {
    let dsl = std::env::var("STELE_POLICY_DSL").ok().unwrap_or_default();
    let mut modules: Vec<std::sync::Arc<dyn PolicyModule>> = Vec::new();
    for spec in dsl.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let mut parts = spec.splitn(2, ':');
        let name = parts.next().unwrap_or("dsl").trim().to_string();
        let rule = parts.next().unwrap_or(spec).trim().to_string();
        modules.push(std::sync::Arc::new(DslPolicyModule { name, rule_src: rule }));
    }
    if modules.is_empty() {
        return None;
    }
    let quorum = std::env::var("STELE_POLICY_QUORUM")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|&q| q > 0)
        .unwrap_or(1);
    Some(ConsensusPolicyEngine::new(modules, quorum))
}
