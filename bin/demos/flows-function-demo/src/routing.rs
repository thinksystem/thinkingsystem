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



#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMode { Native, Plan }

pub struct RouteDecision {
    pub requested: RouteMode,
    pub resolved: RouteMode,
    pub heuristic_triggered: bool,
    pub reason: String,
}

use stele::nlu::llm_processor::LLMAdapter;

pub fn decide_route(
    directive: &str,
    explicit_native: bool,
    explicit_plan: bool,
    has_plan_file: bool,
    auto_requested: bool,
) -> RouteDecision {
    if explicit_plan || has_plan_file {
        return RouteDecision { requested: if auto_requested { RouteMode::Native } else { RouteMode::Plan }, resolved: RouteMode::Plan, heuristic_triggered: false, reason: "explicit-plan".into() };
    }
    if explicit_native {
        return RouteDecision { requested: RouteMode::Native, resolved: RouteMode::Native, heuristic_triggered: false, reason: "explicit-native".into() };
    }
    
    let mut max_num: u64 = 0; let mut _nums = 0u32; let mut cur = String::new();
    for ch in directive.chars() { if ch.is_ascii_digit() { cur.push(ch); } else if !cur.is_empty() { if let Ok(n) = cur.parse::<u64>() { if n > max_num { max_num = n; } _nums += 1;} cur.clear(); } }
    if !cur.is_empty() { if let Ok(n) = cur.parse::<u64>() { if n > max_num { max_num = n; } _nums += 1; } }
    let ops = directive.chars().filter(|c| matches!(c,'+'|'-'|'*'|'/'|'%')).count();
    let assigns = directive.matches('=').count();
    let multi_clause_if = directive.contains("if ") && directive.contains(" else ");
    let recurrence = multi_clause_if && directive.contains("%") && (directive.contains("until 1") || directive.contains(" to 1") || directive.contains(" -> 1"));
    
    let large_space = max_num >= 50_000; 
    let op_dense = ops >= 4; 
    let assign_dense = assigns >= 3; 
    let mut triggers: Vec<String> = Vec::new();
    if large_space { triggers.push(format!("max_num>=50000({max_num})")); }
    if recurrence { triggers.push("recurrence".into()); }
    if op_dense { triggers.push(format!("ops>={ops}")); }
    if assign_dense { triggers.push(format!("assigns>={assigns}")); }
    let plan = !triggers.is_empty();
    if plan {
        RouteDecision { requested: RouteMode::Native, resolved: RouteMode::Plan, heuristic_triggered: true, reason: triggers.join("+") }
    } else {
        RouteDecision { requested: RouteMode::Native, resolved: RouteMode::Native, heuristic_triggered: false, reason: "default-native".into() }
    }
}




pub async fn decide_route_llm(
    adapter: &dyn LLMAdapter,
    directive: &str,
    explicit_native: bool,
    explicit_plan: bool,
    has_plan_file: bool,
) -> RouteDecision {
    if explicit_plan || has_plan_file {
        return RouteDecision { requested: RouteMode::Plan, resolved: RouteMode::Plan, heuristic_triggered: false, reason: "explicit-plan".into() };
    }
    if explicit_native {
        return RouteDecision { requested: RouteMode::Native, resolved: RouteMode::Native, heuristic_triggered: false, reason: "explicit-native".into() };
    }
    let system = r#"Output ONLY JSON: {"route": "native"|"plan", "confidence": 0.0..1.0}
Route selection:
- route=plan when the directive implies multi-step logic, range scanning, search/optimization, graph-like stages, or coordination of multiple evaluators.
- route=native when a single function computation suffices.
Prefer native if uncertain.
No commentary, exact keys."#;
    let user = format!("Directive: {directive}\nDecide.");
    if let Ok(v) = adapter.generate_structured_response(system, &user).await {
        if let Some(route) = v.get("route").and_then(|s| s.as_str()) {
            let conf = v.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.0);
            let resolved = match route {
                "plan" if conf >= 0.5 => RouteMode::Plan,
                _ => RouteMode::Native,
            };
            let requested = RouteMode::Native; 
            return RouteDecision { requested, resolved, heuristic_triggered: true, reason: format!("llm:{route}:{conf:.2}") };
        }
    }
    
    decide_route(directive, explicit_native, explicit_plan, has_plan_file, true)
}
