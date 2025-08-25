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
use stele::nlu::llm_processor::LLMAdapter;

use crate::plan as legacy;

use super::{PlanGenerator, PlanPreprocessor, PlanRepairer, PlanValidator};

pub struct LlmPlanGen<'a> {
    pub adapter: &'a dyn LLMAdapter,
}

#[async_trait]
impl<'a> PlanGenerator for LlmPlanGen<'a> {
    async fn generate(&self, directive: &str) -> Result<Value> {
        legacy::generate_plan_via_llm(self.adapter, directive).await
    }
}

pub struct LlmPlanValidator;
impl PlanValidator for LlmPlanValidator {
    fn validate(&self, plan: &Value) -> Result<()> {
        legacy::validate_plan(plan)
    }
}

pub struct LlmPlanRepairer<'a> {
    pub adapter: &'a dyn LLMAdapter,
}

#[async_trait]
impl<'a> PlanRepairer for LlmPlanRepairer<'a> {
    async fn repair(&self, directive: &str, plan: &Value, error: &str) -> Result<Value> {
        legacy::attempt_repair(self.adapter, directive, plan, error).await
    }
}

pub struct LlmPlanPreprocessor<'a> {
    pub adapter: &'a dyn LLMAdapter,
}

#[async_trait]
impl<'a> PlanPreprocessor for LlmPlanPreprocessor<'a> {
    async fn preprocess(&self, plan: &mut Value, directive: &str, artifacts_dir: &str) -> Result<()> {
        legacy::preprocess_functions(plan, self.adapter, directive, artifacts_dir).await
    }
}
