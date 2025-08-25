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



use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

pub mod llm;

#[async_trait]
pub trait PlanGenerator: Send + Sync {
    async fn generate(&self, directive: &str) -> Result<Value>;
}

pub trait PlanValidator: Send + Sync {
    fn validate(&self, plan: &Value) -> Result<()>;
}

#[async_trait]
pub trait PlanRepairer: Send + Sync {
    async fn repair(&self, directive: &str, plan: &Value, error: &str) -> Result<Value>;
}

#[async_trait]
pub trait PlanPreprocessor: Send + Sync {
    async fn preprocess(&self, plan: &mut Value, directive: &str, artifacts_dir: &str) -> Result<()>;
}

pub struct Planner<G, V, R, P>
where
    G: PlanGenerator,
    V: PlanValidator,
    R: PlanRepairer,
    P: PlanPreprocessor,
{
    gen: G,
    val: V,
    rep: R,
    pre: P,
}

impl<G, V, R, P> Planner<G, V, R, P>
where
    G: PlanGenerator,
    V: PlanValidator,
    R: PlanRepairer,
    P: PlanPreprocessor,
{
    pub fn new(gen: G, val: V, rep: R, pre: P) -> Self {
        Self { gen, val, rep, pre }
    }

    pub async fn generate_validated(
        &self,
        directive: &str,
        max_attempts: u16,
        max_repairs: u8,
    ) -> Result<Value> {
        let mut last_err: Option<String> = None;
        let mut attempts: u16 = 0;
        loop {
            attempts += 1;
            if attempts > max_attempts {
                if let Some(e) = last_err { return Err(anyhow::anyhow!(e)); }
                return Err(anyhow::anyhow!("Exceeded plan attempts"));
            }
            let plan = match self.gen.generate(directive).await {
                Ok(p) => p,
                Err(e) => { last_err = Some(e.to_string()); continue; }
            };
            if let Err(e) = self.val.validate(&plan) {
                last_err = Some(e.to_string());
                
                let mut r = 0u8; let mut repaired_ok: Option<Value> = None;
                while r < max_repairs {
                    r += 1;
                    match self.rep.repair(directive, &plan, &e.to_string()).await {
                        Ok(p2) => {
                            if self.val.validate(&p2).is_ok() { repaired_ok = Some(p2); break; }
                        }
                        Err(er) => { last_err = Some(er.to_string()); }
                    }
                }
                if let Some(ok) = repaired_ok { return Ok(ok); }
                continue;
            }
            return Ok(plan);
        }
    }

    pub async fn preprocess(
        &self,
        plan: &mut Value,
        directive: &str,
        artifacts_dir: &str,
    ) -> Result<()> {
        self.pre.preprocess(plan, directive, artifacts_dir).await
    }
}
