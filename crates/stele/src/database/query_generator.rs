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

use crate::database::intent_analyser::ToolCall;
use crate::database::query_builder::{
    Condition, GraphPathSegment, GraphTraversal, Operator, OrderClause, OrderDirection,
    PathDirection, SelectQuery,
};
use crate::database::schema_analyser::GraphSchema;
use crate::nlu::orchestrator::data_models::{AdvancedQueryIntent, TraversalInfo};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};
pub struct AdvancedQueryGenerator {
    graph_schema: Arc<RwLock<GraphSchema>>,
}
impl AdvancedQueryGenerator {
    pub fn new(graph_schema: Arc<RwLock<GraphSchema>>) -> Self {
        Self { graph_schema }
    }
    pub async fn build_query_from_intent(
        &self,
        intent: &AdvancedQueryIntent,
    ) -> Result<SelectQuery, String> {
        debug!("Building query directly from intent: {:?}", intent);
        let mut query = SelectQuery::new().from(vec!["nodes".to_string()]);
        let mut all_conditions: Option<Condition> = None;
        if !intent.entities.is_empty() {
            let schema_guard = self.graph_schema.read().await;
            let search_fields = schema_guard
                .node_search_patterns
                .get("any entity type or category")
                .cloned()
                .unwrap_or_else(|| vec!["type".to_string()]);
            let mut entity_conditions: Option<Condition> = None;
            for entity_type in &intent.entities {
                for field in &search_fields {
                    let cond = Condition::simple(field, Operator::Equals, json!(entity_type));
                    entity_conditions = match entity_conditions {
                        Some(existing) => Some(existing.or(cond)),
                        None => Some(cond),
                    };
                }
            }
            if let Some(cond) = entity_conditions {
                all_conditions = Some(cond.group());
            }
        }
        if !intent.filters.is_empty() {
            for (field, value) in &intent.filters {
                let filter_condition = Condition::simple(field, Operator::Contains, value.clone());
                all_conditions = match all_conditions {
                    Some(existing) => Some(existing.and(filter_condition)),
                    None => Some(filter_condition),
                };
            }
        }
        if let Some(conditions) = all_conditions {
            query = query.where_condition(conditions);
        }
        query = query.fields(vec!["*".to_string()]);
        Ok(query)
    }
    pub async fn build_query_from_plan(
        &self,
        plan: &[ToolCall],
        intent: Option<&AdvancedQueryIntent>,
    ) -> Result<SelectQuery, String> {
        let mut query = SelectQuery::new().from(vec!["nodes".to_string()]);
        if plan.is_empty() && intent.is_none() {
            return Err(
                "The query plan and intent are both empty. Cannot generate a query.".to_string(),
            );
        }
        for tool_call in plan {
            debug!("Applying tool call to query: {}", tool_call.name);
            query = self.apply_tool_call(query, tool_call).await?;
        }
        if let Some(intent_data) = intent {
            query = self.apply_intent_traversals(query, intent_data).await?;
        }
        if query.fields_ref().is_empty() {
            query = query.fields(vec!["*".to_string()]);
        }
        Ok(query)
    }
    async fn apply_intent_traversals(
        &self,
        mut query: SelectQuery,
        intent: &AdvancedQueryIntent,
    ) -> Result<SelectQuery, String> {
        if intent.traversals.is_empty() {
            return Ok(query);
        }
        let mut combined_condition: Option<Condition> = None;
        for traversal in &intent.traversals {
            let new_condition = self
                .build_single_traversal_condition(traversal, &intent.entities)
                .await?;
            if let Some(existing) = combined_condition {
                combined_condition = Some(existing.and(new_condition));
            } else {
                combined_condition = Some(new_condition);
            }
        }
        if let Some(condition) = combined_condition {
            query = query.where_condition(condition);
        }
        Ok(query)
    }
    async fn build_single_traversal_condition(
        &self,
        traversal: &TraversalInfo,
        entities: &[String],
    ) -> Result<Condition, String> {
        if traversal.via_relationships.is_empty() {
            return Err("Traversal must specify at least one relationship".to_string());
        }
        let hops = traversal.hops;
        if hops == 0 {
            return Err("Traversal hops must be greater than zero".to_string());
        }
        let relationships = &traversal.via_relationships;
        let direction = &traversal.direction;
        let mut segments: Vec<GraphPathSegment> = Vec::new();
        for i in 0..hops {
            let rel = relationships
                .get(i as usize)
                .unwrap_or_else(|| relationships.last().unwrap());
            let path_direction = match direction.to_lowercase().as_str() {
                "outgoing" | "out" | "" => PathDirection::Outbound,
                "incoming" | "in" => PathDirection::Inbound,
                "any" | "both" | "bidirectional" => PathDirection::Bidirectional,
                _ => return Err(format!("Invalid traversal direction: {direction}")),
            };
            let is_last_hop = (i + 1) == hops;
            let mut segment = GraphPathSegment {
                direction: path_direction,
                edge_table: rel.to_string(),
                target_node_table: Some("nodes".to_string()),
                conditions: None,
            };
            if is_last_hop && !entities.is_empty() {
                let schema_guard = self.graph_schema.read().await;
                let all_search_fields: Vec<String> = schema_guard
                    .node_search_patterns
                    .values()
                    .flatten()
                    .cloned()
                    .collect();
                if all_search_fields.is_empty() {
                    return Err("No node search patterns are defined in the schema.".to_string());
                }
                let mut target_condition: Option<Condition> = None;
                for entity_name in entities {
                    for field in &all_search_fields {
                        let new_condition =
                            Condition::simple(field, Operator::Contains, json!(entity_name));
                        target_condition = match target_condition {
                            Some(existing) => Some(existing.or(new_condition)),
                            None => Some(new_condition),
                        };
                    }
                }
                segment.conditions = target_condition.map(|c| c.group());
            }
            segments.push(segment);
        }
        let traversal_condition = Condition::GraphTraversal(GraphTraversal { segments });
        Ok(traversal_condition)
    }
    async fn apply_tool_call(
        &self,
        mut query: SelectQuery,
        tool_call: &ToolCall,
    ) -> Result<SelectQuery, String> {
        match tool_call.name.as_str() {
            "find_entities" => {
                let search_term = get_required_string(&tool_call.arguments, "search_term")?;
                let schema_guard = self.graph_schema.read().await;
                let all_search_fields: Vec<String> = schema_guard
                    .node_search_patterns
                    .values()
                    .flatten()
                    .cloned()
                    .collect();
                if all_search_fields.is_empty() {
                    return Err("No node search patterns are defined in the schema.".to_string());
                }
                let mut conditions: Option<Condition> = None;
                for field in all_search_fields {
                    let new_condition =
                        Condition::simple(&field, Operator::Contains, json!(search_term));
                    conditions = match conditions {
                        Some(existing) => Some(existing.or(new_condition)),
                        None => Some(new_condition),
                    };
                }
                query = query.where_condition(conditions.unwrap().group());
            }
            "filter_by_confidence" => {
                let min_score = get_required_number(&tool_call.arguments, "min_score")?;
                query = query.where_greater_than("properties.confidence", json!(min_score));
            }
            "set_result_limit" => {
                let count = get_required_number(&tool_call.arguments, "count")? as u64;
                query = query.limit(count);
            }
            "set_result_order" => {
                let field = get_required_string(&tool_call.arguments, "field")?;
                let direction_str = get_required_string(&tool_call.arguments, "direction")?;
                let direction = match direction_str.to_uppercase().as_str() {
                    "ASC" => OrderDirection::Asc,
                    "DESC" => OrderDirection::Desc,
                    _ => return Err(format!("Invalid sort direction: {direction_str}")),
                };
                query = query.order_by(vec![OrderClause::new(field.to_string(), direction)]);
            }
            "filter_by_relationship" => {
                warn!("'filter_by_relationship' tool is legacy; using intent-based traversal is preferred.");
                let relationship_type =
                    get_required_string(&tool_call.arguments, "relationship_type")?;
                let target_entity_name =
                    get_required_string(&tool_call.arguments, "target_entity_name")?;
                let traversal_info = TraversalInfo {
                    hops: 1,
                    direction: "outgoing".to_string(),
                    via_relationships: vec![relationship_type.to_string()],
                };
                let entities = vec![target_entity_name.to_string()];
                let condition = self
                    .build_single_traversal_condition(&traversal_info, &entities)
                    .await?;
                query = query.where_condition(condition);
            }
            "filter_by_temporal" => {
                warn!("Tool 'filter_by_temporal' is defined in configuration but not yet implemented in the query generator. Skipping this filter.");
            }
            "request_source_traceability" => {
                warn!("Tool 'request_source_traceability' is defined in configuration but not yet implemented in the query generator. Skipping this request.");
            }
            _ => {
                warn!(
                    "Encountered an unknown tool in the plan: '{}'. Skipping.",
                    tool_call.name
                );
            }
        }
        Ok(query)
    }
}
fn get_required_string<'a>(args: &'a HashMap<String, Value>, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Missing or invalid string argument: '{key}'"))
}
fn get_required_number(args: &HashMap<String, Value>, key: &str) -> Result<f64, String> {
    args.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("Missing or invalid number argument: '{key}'"))
}
