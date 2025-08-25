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


use crate::ir::{ir_to_wat, IRFunction, IRNode};
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use stele::codegen::artifacts::persist_wat_artifact;
use stele::nlu::llm_processor::LLMAdapter;
use tracing::warn;

pub async fn generate_plan_via_llm(
    adapter: &dyn LLMAdapter,
    directive: &str,
) -> anyhow::Result<Value> {
    let system = r#"You output ONLY strict JSON plan for a dynamic numeric function + flow (+ optional execution_graph + required evaluator artifacts when a graph is present).
Base Minimal Schema:
{
    "functions": [ { "name": "snake_case", "export": "same", "ir": {"params":[{"name":"n","type":"f64"}], "locals":[], "body": [{"op":"F64_CONST","value":1.0}] } } ],
    "flow": {"id":"generated_flow","start":"intro","blocks":[{"id":"intro","type":"display","message":"Processing directive: {directive}","next":"run"},{"id":"run","type":"compute","expression":"function:<function_name>","args":[123],"output_key":"result","next":"end"},{"id":"end","type":"terminal"}]},
    "execution_graph": {"nodes": [ {"type":"range_scan","id":"scan0","evaluator":"seq_eval_v1","start":1,"end":1000000,"prefer_dense_cutoff":120000000,"shards":64,"chunk":1000000,"progress_log_interval":250000} ], "metadata": {} },
    "evaluators": [ { "id":"seq_eval_v1", "type":"dsl", "source":"rule n % 2 == 0 -> n = n / 2\nrule n % 2 == 1 -> n = n * 3; n = n + 1\nrule n == 1 -> terminate" } ]
}
STRICT RULES:
- Return ONLY JSON (no markdown, no commentary) and ensure top-level is an object.
- When directive implies scanning a numeric range (keywords: longest, largest, max, under, search, chain length, sequence length), MUST include execution_graph + evaluators. Prefer evaluator type "function" that references a simple per-n predicate function you include in functions[]; fallback to type "dsl" if appropriate.
- execution_graph.nodes: array; supported node type: "range_scan" with fields: id, evaluator, start, end, prefer_dense_cutoff, shards, chunk, progress_log_interval, early_stop_no_improve (optional). Evaluator IDs are opaque (descriptive names allowed).
- Every evaluator referenced in execution_graph MUST have a corresponding entry in top-level "evaluators" array. For type "function", include fields {"id":..., "type":"function", "function":"<function_name>", "prefer_min_n":true}.
- Prefer a two-stage "switch_scan" when scanning is implied: stage 1 uses a coarse evaluator (type "function") to quickly filter candidates; stage 2 uses a more precise evaluator ("function" or concise "dsl") to refine. Provide both evaluator entries and reference them in order via switch_scan.evaluators.
- For sequence length style tasks, supply a DSL evaluator using only rule lines; DSL grammar lines (order significant, SINGLE predicate per rule — do NOT use 'and', '&&', '||'):
        rule n % <mod> == <eq> -> <one_or_more_ops_separated_by_semicolons>
        rule n % <mod> == <eq> -> terminate
        rule n == <const> -> terminate
    Allowed rule ACTION tokens ONLY (sequence separated by semicolons):
        n = n / <int>
        n = n * <int>
        n = n + <int>
        n = n - <int>
        terminate
    NO other action tokens (FORBIDDEN examples: terminate_and_record_run, record, emit, output, capture, save).
    Provide at least one terminating rule (e.g., n == 1) and transformation rules that change n.
    If you conceptually need conjunction (e.g. divisible by 6), express directly with modulus 6 instead of chaining conditions.
- No implicit defaults: ALL logic must be inside the DSL source string you output.
- Use IR (field 'ir') for function bodies when simple arithmetic/loop logic fits allowed op set; otherwise directly supply 'wat'.
- Single function usually sufficient.
    Single function usually sufficient. If directive implies a large search (keywords: scan, consecutive, run, sequence, composite, factor), function MUST NOT just return a constant; include at least one loop or arithmetic chain.
- Choose a numeric arg from directive; if none, use 1000.
- Allowed IR op codes: F64_CONST, I64_CONST, I32_CONST, F64_ADD, F64_SUB, F64_MUL, F64_DIV, F64_SQRT, GET_LOCAL, SET_LOCAL, I64_ADD, I64_SUB, I64_MUL, I64_REM_S, I64_GT_S, I64_LT_S, I64_EQZ, I32_EQ, I64_TRUNC_F64_S, F64_CONVERT_I64_S, F64_SQRT, BLOCK, LOOP, BR, BR_IF, IF, RETURN.
- Do NOT invent operations outside the list.
- Keep numeric fields as numbers, not strings.
- Names may be descriptive; engine treats them as opaque (no special meaning inferred).
"#;
    let user = format!("Directive: {directive}\nProduce plan JSON now.");
    let raw = adapter
        .generate_structured_response(system, &user)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "LLM plan generation failed: {e} (ensure local endpoint/model are running)"
            )
        })?;
    let enriched = raw.clone();
    let mut numeric_limit: Option<u64> = None;
    for tok in directive.split(|c: char| !c.is_ascii_digit()) {
        if tok.is_empty() {
            continue;
        }
        if let Ok(v) = tok.parse::<u64>() {
            numeric_limit = Some(numeric_limit.map(|cur| cur.max(v)).unwrap_or(v));
        }
    }

    if enriched.get("functions").is_some() && enriched.get("flow").is_some() {
        return Ok(enriched);
    }
    for (_, v) in raw.as_object().into_iter().flat_map(|m| m.iter()) {
        if let Some(s) = v.as_str() {
            if let Ok(candidate) = serde_json::from_str::<Value>(s) {
                if candidate.get("functions").is_some() && candidate.get("flow").is_some() {
                    return Ok(candidate);
                }
            }
        }
    }
    Err(anyhow::anyhow!("LLM did not produce expected plan schema"))
}

pub fn validate_plan(plan: &Value) -> anyhow::Result<()> {
    if !plan.is_object() {
        return Err(anyhow::anyhow!("Plan not an object"));
    }
    let f = plan
        .get("functions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("Missing 'functions' array"))?;
    if f.is_empty() {
        return Err(anyhow::anyhow!("Empty functions array"));
    }

    let mut fn_names: Vec<String> = Vec::new();
    for func in f {
        if func
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
        {
            return Err(anyhow::anyhow!("Function missing name"));
        }
        if let Some(n) = func.get("name").and_then(|v| v.as_str()) {
            fn_names.push(n.to_string());
        }
        if !(func.get("wat").is_some() || func.get("ir").is_some()) {
            return Err(anyhow::anyhow!("Function missing 'wat' or 'ir'"));
        }
    }
    let flow = plan
        .get("flow")
        .ok_or_else(|| anyhow::anyhow!("Missing flow"))?;
    flow.get("blocks")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Flow missing blocks"))?;

    if let Some(blocks) = flow.get("blocks").and_then(|v| v.as_array()) {
        for b in blocks {
            if b.get("type").and_then(|v| v.as_str()) == Some("compute") {
                if let Some(expr) = b.get("expression").and_then(|v| v.as_str()) {
                    if let Some(rest) = expr.strip_prefix("function:") {
                        let target = rest.trim();
                        if !fn_names.iter().any(|n| n == target) {
                            return Err(anyhow::anyhow!(format!(
                                "Flow references missing function '{target}'"
                            )));
                        }
                    }
                }
            }
        }
    }

    if let Some(exec_graph) = plan.get("execution_graph") {
        validate_execution_graph(exec_graph)?;

        let evals_vec: Vec<Value> = plan
            .get("evaluators")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let eval_ids: Vec<String> = evals_vec
            .iter()
            .filter_map(|e| e.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect();

        for e in &evals_vec {
            if e.get("type").and_then(|v| v.as_str()) == Some("function") {
                if let Some(fun) = e.get("function").and_then(|v| v.as_str()) {
                    if !fn_names.iter().any(|n| n == fun) {
                        return Err(anyhow::anyhow!(format!(
                            "Evaluator '{}' references missing function '{}'",
                            e.get("id").and_then(|v| v.as_str()).unwrap_or("<unknown>"),
                            fun
                        )));
                    }
                } else {
                    return Err(anyhow::anyhow!(
                        "Function evaluator missing 'function' field"
                    ));
                }
            }
        }

        let mut referenced_eval_ids: Vec<String> = Vec::new();
        if let Some(nodes) = exec_graph.get("nodes").and_then(|v| v.as_array()) {
            for n in nodes {
                match n.get("type").and_then(|v| v.as_str()) {
                    Some("range_scan") => {
                        if let Some(ev) = n.get("evaluator").and_then(|v| v.as_str()) {
                            if !eval_ids.iter().any(|id| id == ev) {
                                return Err(anyhow::anyhow!(format!(
                                    "range_scan node references unknown evaluator id '{}'",
                                    ev
                                )));
                            }
                            referenced_eval_ids.push(ev.to_string());
                        }
                    }
                    Some("switch_scan") => {
                        if let Some(arr) = n.get("evaluators").and_then(|v| v.as_array()) {
                            for ev in arr.iter().filter_map(|v| v.as_str()) {
                                if !eval_ids.iter().any(|id| id == ev) {
                                    return Err(anyhow::anyhow!(format!(
                                        "switch_scan node references unknown evaluator id '{}'",
                                        ev
                                    )));
                                }
                                referenced_eval_ids.push(ev.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        for id in &eval_ids {
            if !referenced_eval_ids.iter().any(|r| r == id) {
                warn!(evaluator=%id, "Evaluator declared but not referenced by any execution_graph node");
            }
        }

        let mut md_switch = false;
        if let Some(md) = exec_graph.get("metadata").and_then(|v| v.as_object()) {
            if md.get("switch_scan").is_some() {
                md_switch = true;
            }
        }
        if md_switch {
            let mut has_switch = false;
            let mut switch_eval_count = 0usize;
            if let Some(nodes) = exec_graph.get("nodes").and_then(|v| v.as_array()) {
                for n in nodes {
                    if n.get("type").and_then(|v| v.as_str()) == Some("switch_scan") {
                        has_switch = true;
                        switch_eval_count = n
                            .get("evaluators")
                            .and_then(|v| v.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0);
                    }
                }
            }
            if !has_switch || switch_eval_count < 2 {
                warn!(
                    "execution_graph metadata suggests switch_scan; allowing single-stage for now — upgrade may occur during preprocess"
                );
            }
        }
    }

    Ok(())
}

pub async fn attempt_repair(
    adapter: &dyn LLMAdapter,
    directive: &str,
    plan: &Value,
    error: &str,
) -> anyhow::Result<Value> {
    let system =
        r#"You fix an invalid plan JSON. Output ONLY corrected full plan JSON, no commentary."#;
    let user = format!(
        "Directive: {directive}\nValidationError: {error}\nPriorPlan: {plan}\nReturn corrected full plan JSON now."
    );
    let resp = adapter
        .generate_structured_response(system, &user)
        .await
        .map_err(|e| anyhow::anyhow!("LLM repair failed: {e}"))?;
    Ok(resp)
}

pub async fn preprocess_functions(
    plan: &mut Value,
    adapter: &dyn LLMAdapter,
    directive: &str,
    artifacts_dir: &str,
) -> anyhow::Result<()> {
    preprocess_functions_dual(plan, adapter, adapter, directive, artifacts_dir).await
}

fn validate_execution_graph(graph: &Value) -> anyhow::Result<()> {
    if !graph.is_object() {
        return Err(anyhow::anyhow!("execution_graph not object"));
    }
    let nodes = graph
        .get("nodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("execution_graph.nodes missing"))?;
    if nodes.is_empty() {
        return Err(anyhow::anyhow!("execution_graph.nodes empty"));
    }
    for n in nodes {
        let t = n
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("node.type missing"))?;
        match t {
            "range_scan" => {
                for field in ["id", "evaluator"] {
                    if n.get(field)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .is_empty()
                    {
                        return Err(anyhow::anyhow!(format!("range_scan node missing {field}")));
                    }
                }
                for num in [
                    "start",
                    "end",
                    "prefer_dense_cutoff",
                    "shards",
                    "chunk",
                    "progress_log_interval",
                ] {
                    if n.get(num).and_then(|v| v.as_u64()).is_none() {
                        return Err(anyhow::anyhow!(format!(
                            "range_scan node missing numeric field {num}"
                        )));
                    }
                }
            }
            "switch_scan" => {
                if n.get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .is_empty()
                {
                    return Err(anyhow::anyhow!("switch_scan node missing id"));
                }
                let evals = n
                    .get("evaluators")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| anyhow::anyhow!("switch_scan node evaluators missing"))?;
                if evals.is_empty() {
                    return Err(anyhow::anyhow!("switch_scan node evaluators empty"));
                }
                for e in evals {
                    if e.as_str().unwrap_or("").is_empty() {
                        return Err(anyhow::anyhow!("switch_scan evaluator empty"));
                    }
                }
                for num in [
                    "start",
                    "end",
                    "prefer_dense_cutoff",
                    "shards",
                    "chunk",
                    "progress_log_interval",
                ] {
                    if n.get(num).and_then(|v| v.as_u64()).is_none() {
                        return Err(anyhow::anyhow!(format!(
                            "switch_scan node missing numeric field {num}"
                        )));
                    }
                }
                if let Some(th) = n.get("stage_advance_min_improve") {
                    if th.as_u64().is_none() {
                        return Err(anyhow::anyhow!(
                            "switch_scan stage_advance_min_improve not u64"
                        ));
                    }
                }
            }
            other => {
                return Err(anyhow::anyhow!(format!(
                    "unsupported execution_graph node type {other}"
                )));
            }
        }
    }
    Ok(())
}

pub async fn preprocess_functions_dual(
    plan: &mut Value,
    adapter: &dyn LLMAdapter,
    fallback_adapter: &dyn LLMAdapter,
    directive: &str,
    artifacts_dir: &str,
) -> anyhow::Result<()> {
    if let Some(funcs) = plan.get_mut("functions").and_then(|v| v.as_array_mut()) {
        for f in funcs.iter_mut() {
            let mut replace_ir: Option<Value> = None;
            if f.get("wat").is_some() {
                continue;
            }
            if let Some(ir_val) = f.get("ir") {
                let mut ir_clone = ir_val.clone();
                let params_clone = ir_clone.get("params").cloned();
                let locals_clone = ir_clone.get("locals").cloned();
                if let Some(body) = ir_clone.get_mut("body") {
                    normalize_ir_indices(body, params_clone.as_ref(), locals_clone.as_ref());
                }
                replace_ir = Some(ir_clone);
                if let Some(body) = ir_val.get("body").and_then(|v| v.as_array()) {
                    const ALLOWED_IR_OPS: &[&str] = &[
                        "F64_CONST",
                        "I64_CONST",
                        "I32_CONST",
                        "F64_ADD",
                        "F64_SUB",
                        "F64_MUL",
                        "F64_DIV",
                        "F64_SQRT",
                        "GET_LOCAL",
                        "SET_LOCAL",
                        "I64_ADD",
                        "I64_SUB",
                        "I64_MUL",
                        "I64_REM_S",
                        "I64_GT_S",
                        "I64_LT_S",
                        "I64_EQZ",
                        "I32_EQ",
                        "I64_TRUNC_F64_S",
                        "F64_CONVERT_I64_S",
                        "BLOCK",
                        "LOOP",
                        "BR",
                        "BR_IF",
                        "IF",
                        "RETURN",
                    ];
                    let mut bad: Vec<String> = Vec::new();
                    for node in body {
                        if let Some(op) = node.get("op").and_then(|v| v.as_str()) {
                            if !ALLOWED_IR_OPS.contains(&op) {
                                bad.push(op.to_string());
                            }
                        }
                    }
                    if !bad.is_empty() {
                        f.as_object_mut().unwrap().remove("ir");
                    }
                }
                if let Some(ir_val2) = f.get("ir") {
                    if let Ok(ir_fn) = serde_json::from_value::<IRFunction>(ir_val2.clone()) {
                        let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("gen_fn");
                        let export = f.get("export").and_then(|v| v.as_str()).unwrap_or(name);
                        if generic_ir_quality(&ir_fn) {
                            let wat = ir_to_wat(name, export, &ir_fn);
                            let name_for_persist = name.to_string();
                            f.as_object_mut()
                                .unwrap()
                                .insert("wat".into(), Value::String(wat.clone()));
                            let _ = persist_wat_artifact(
                                artifacts_dir,
                                &name_for_persist,
                                &wat,
                                "pre_exec_ir_translate_dual",
                            );
                            continue;
                        } else {
                            f.as_object_mut().unwrap().remove("ir");
                        }
                    } else {
                        f.as_object_mut().unwrap().remove("ir");
                    }
                }
            }

            let fname = f
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("gen_fn")
                .to_string();
            let export = f
                .get("export")
                .and_then(|v| v.as_str())
                .unwrap_or(&fname)
                .to_string();
            let system = "You output ONLY JSON with a single key 'wat' containing a minimal valid WebAssembly Text module exporting the target function.";
            let user = format!(
                "Directive: {directive}\nFunction name: {fname}\nExport: {export}\nReturn JSON now."
            );
            let mut wat_opt: Option<String> = None;

            if let Ok(resp) = adapter.generate_structured_response(system, &user).await {
                if let Some(wat_str) = resp.get("wat").and_then(|v| v.as_str()) {
                    wat_opt = Some(wat_str.to_string());
                }
            }

            if wat_opt.is_none() {
                if let Ok(resp) = fallback_adapter
                    .generate_structured_response(system, &user)
                    .await
                {
                    if let Some(wat_str) = resp.get("wat").and_then(|v| v.as_str()) {
                        wat_opt = Some(wat_str.to_string());
                    }
                }
            }
            if let Some(wat_str) = wat_opt {
                f.as_object_mut()
                    .unwrap()
                    .insert("wat".into(), Value::String(wat_str.clone()));
                let _ =
                    persist_wat_artifact(artifacts_dir, &fname, &wat_str, "pre_exec_llm_gen_dual");
            }
            if let Some(new_ir) = replace_ir {
                if let Some(map) = f.as_object_mut() {
                    map.insert("ir".into(), new_ir);
                }
            }
        }
    }
    Ok(())
}

pub fn enforce_dual_stage_scan(plan: &mut Value) {
    let nodes_ro = plan
        .get("execution_graph")
        .and_then(|v| v.get("nodes"))
        .and_then(|v| v.as_array())
        .cloned();
    let mut first_eval: Option<String> = None;
    let mut start: u64 = 1;
    let mut end: u64 = 1_000_000;
    let mut prefer_dense_cutoff: u64 = 120_000_000;
    let mut shards: u64 = 64;
    let mut chunk: u64 = 1_000_000;
    let mut progress_log_interval: u64 = 250_000;
    if let Some(nodes) = &nodes_ro {
        if let Some(first) = nodes.first().and_then(|n| n.as_object()) {
            if first.get("type").and_then(|v| v.as_str()) == Some("range_scan") {
                if let Some(ev) = first.get("evaluator").and_then(|v| v.as_str()) {
                    first_eval = Some(ev.to_string());
                }
                start = first.get("start").and_then(|v| v.as_u64()).unwrap_or(start);
                end = first.get("end").and_then(|v| v.as_u64()).unwrap_or(end);
                prefer_dense_cutoff = first
                    .get("prefer_dense_cutoff")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(prefer_dense_cutoff);
                shards = first
                    .get("shards")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(shards);
                chunk = first.get("chunk").and_then(|v| v.as_u64()).unwrap_or(chunk);
                progress_log_interval = first
                    .get("progress_log_interval")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(progress_log_interval);
            }
        }
    } else {
        return;
    }

    let mut eval_ids: Vec<String> = Vec::new();
    let mut synth_second: Option<(String, String)> = None;
    if let Some(evals) = plan.get("evaluators").and_then(|v| v.as_array()) {
        for e in evals.iter() {
            if let Some(id) = e.get("id").and_then(|v| v.as_str()) {
                eval_ids.push(id.to_string());
            }
        }
        if evals.len() == 1 {
            if let (Some(fid), Some(fun)) = (
                evals[0].get("id").and_then(|v| v.as_str()),
                evals[0].get("function").and_then(|v| v.as_str()),
            ) {
                synth_second = Some((fid.to_string(), fun.to_string()));
            }
        }
    }

    if let Some((fid, fun)) = synth_second.take() {
        let fine_id = format!("{fid}_fine");
        let fine = serde_json::json!({
            "id": fine_id,
            "type": "function",
            "function": fun,
            "prefer_min_n": true
        });
        if plan.get("evaluators").is_none() {
            if let Some(obj) = plan.as_object_mut() {
                obj.insert("evaluators".into(), serde_json::json!([]));
            }
        }
        if let Some(arr) = plan.get_mut("evaluators").and_then(|v| v.as_array_mut()) {
            arr.push(fine);
            eval_ids.push(format!("{fid}_fine"));
        }
    }
    if eval_ids.len() >= 2 {
        let sw = serde_json::json!({
            "type": "switch_scan",
            "id": "switch0",
            "evaluators": [eval_ids[0].clone(), eval_ids[1].clone()],
            "start": start,
            "end": end,
            "prefer_dense_cutoff": prefer_dense_cutoff,
            "shards": shards,
            "chunk": chunk,
            "progress_log_interval": progress_log_interval,
            "stage_advance_min_improve": 1
        });
        if let Some(graph_obj) = plan
            .get_mut("execution_graph")
            .and_then(|v| v.as_object_mut())
        {
            graph_obj.insert("nodes".into(), serde_json::json!([sw]));
        }
    } else if let Some(fe) = first_eval {
        let exists = plan
            .get("evaluators")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .any(|e| e.get("id").and_then(|v| v.as_str()) == Some(&fe))
            })
            .unwrap_or(false);
        if !exists {
            if let Some(fun_name) = plan
                .get("functions")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|f| f.get("name").and_then(|v| v.as_str()))
            {
                let mut new_evals = plan
                    .get("evaluators")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                new_evals.push(serde_json::json!({"id": fe, "type":"function", "function": fun_name, "prefer_min_n": true}));
                if let Some(obj) = plan.as_object_mut() {
                    obj.insert("evaluators".into(), Value::Array(new_evals));
                }
            }
        }
    }
}

pub async fn enforce_dual_stage_scan_llm(
    plan: &mut Value,
    adapter: &dyn LLMAdapter,
    directive: &str,
) {
    let mut graph_hint = serde_json::json!({
        "has_execution_graph": plan.get("execution_graph").is_some(),
        "node_types": [],
        "evaluator_count": plan.get("evaluators").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
    });
    if let Some(nodes) = plan
        .get("execution_graph")
        .and_then(|v| v.get("nodes"))
        .and_then(|v| v.as_array())
    {
        let kinds: Vec<String> = nodes
            .iter()
            .filter_map(|n| {
                n.get("type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        graph_hint
            .as_object_mut()
            .unwrap()
            .insert("node_types".into(), serde_json::json!(kinds));
    }

    let system = r#"You are a strict classifier. Output ONLY JSON like:
{"scan_like": true|false, "enforce_dual_stage": true|false, "confidence": 0.0..1.0}
Guidelines:
- scan_like means the directive inherently requires scanning/iterating across a large numeric range.
- enforce_dual_stage means upgrading a single-stage range_scan to a two-stage switch_scan will measurably help.
- Prefer false if uncertain. No commentary, no markdown, keys exactly as specified."#;
    let user = format!("Directive: {directive}\nPlanGraphHint: {graph_hint}\nDecide now.");
    let mut allow_upgrade = false;
    if let Ok(resp) = adapter.generate_structured_response(system, &user).await {
        let scan_like = resp
            .get("scan_like")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let enforce = resp
            .get("enforce_dual_stage")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let conf = resp
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        allow_upgrade = (enforce && conf >= 0.5) || (scan_like && conf >= 0.7);
    }
    if allow_upgrade {
        enforce_dual_stage_scan(plan);
    }
}

pub async fn assess_plan_feasibility(
    adapter: &dyn LLMAdapter,
    directive: &str,
    plan: &Value,
    artifacts_dir: Option<&str>,
) -> Option<serde_json::Value> {
    let system = r#"You are a strict feasibility reviewer. Output ONLY JSON with keys:
{"feasible": true|false, "confidence": 0.0..1.0, "reasons": [string], "missing": [string], "suggested_repairs": [string]}
Guidelines:
- Judge if the provided plan (functions + flow + optional execution_graph + evaluators) could plausibly satisfy the directive.
- Do not execute; reason about structure: presence of scanning for large search tasks, evaluators wired, functions provided, outputs captured.
- No markdown, no commentary outside the JSON. Keys exactly as specified."#;
    let user = format!("Directive: {directive}\nPlan: {plan}\nDecide now.");
    match adapter.generate_structured_response(system, &user).await {
        Ok(val) => {
            if let Some(dir) = artifacts_dir {
                let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S");
                let path = format!("{dir}/feasibility_{ts}.json");
                if let Ok(mut f) = File::create(&path) {
                    let _ = f.write_all(val.to_string().as_bytes());
                }
            }
            Some(val)
        }
        Err(_) => None,
    }
}

pub async fn feasibility_gate(
    adapter: &dyn LLMAdapter,
    directive: &str,
    plan: &mut Value,
    attempts: u8,
    artifacts_dir: Option<&str>,
) {
    let mut tries = 0u8;
    while tries < attempts {
        tries += 1;
        let report = assess_plan_feasibility(adapter, directive, plan, artifacts_dir).await;
        if let Some(rep) = report {
            let feasible = rep
                .get("feasible")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let conf = rep
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            if feasible && conf >= 0.4 {
                break;
            }

            let mut nudged = false;
            if plan.get("execution_graph").is_some() {
                let func_name = plan
                    .get("functions")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let eval_id = "eval_auto_v1";
                let evaluators_missing_or_empty = plan
                    .get("evaluators")
                    .and_then(|v| v.as_array())
                    .map(|a| a.is_empty())
                    .unwrap_or(true);
                if evaluators_missing_or_empty {
                    let mut evals = plan
                        .get("evaluators")
                        .cloned()
                        .unwrap_or_else(|| Value::Array(vec![]));
                    if let Some(arr) = evals.as_array_mut() {
                        if let Some(fname) = &func_name {
                            arr.push(serde_json::json!({
                                "id": eval_id,
                                "type": "function",
                                "function": fname,
                                "prefer_min_n": true
                            }));
                            nudged = true;
                        }
                    }
                    plan.as_object_mut()
                        .unwrap()
                        .insert("evaluators".into(), evals);
                }

                let have_evaluators_now = plan
                    .get("evaluators")
                    .and_then(|v| v.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false);

                if let Some(nodes) = plan
                    .get_mut("execution_graph")
                    .and_then(|v| v.get_mut("nodes"))
                    .and_then(|v| v.as_array_mut())
                {
                    for n in nodes {
                        if let Some(t) = n.get("type").and_then(|v| v.as_str()) {
                            if t == "range_scan" {
                                if n.get("evaluator").is_none() && have_evaluators_now {
                                    n.as_object_mut().unwrap().insert(
                                        "evaluator".into(),
                                        Value::String(eval_id.to_string()),
                                    );
                                    nudged = true;
                                }
                            } else if t == "switch_scan" {
                                let missing_or_empty = n
                                    .get("evaluators")
                                    .and_then(|v| v.as_array())
                                    .map(|a| a.is_empty())
                                    .unwrap_or(true);
                                if missing_or_empty && have_evaluators_now {
                                    n.as_object_mut().unwrap().insert(
                                        "evaluators".into(),
                                        Value::Array(vec![Value::String(eval_id.to_string())]),
                                    );
                                    nudged = true;
                                }
                            }
                        }
                    }
                }
            }
            if nudged {
                continue;
            }
        }

        break;
    }
}

fn normalize_ir_indices(node: &mut Value, params: Option<&Value>, locals: Option<&Value>) {
    match node {
        Value::Array(arr) => {
            for elem in arr {
                normalize_ir_indices(elem, params, locals);
            }
        }
        Value::Object(map) => {
            if let Some(op_str_raw) = map.get("op").and_then(|v| v.as_str()) {
                let op_str = op_str_raw.to_string();
                if (op_str.as_str() == "GET_LOCAL"
                    || op_str.as_str() == "SET_LOCAL"
                    || op_str.as_str() == "BR"
                    || op_str.as_str() == "BR_IF")
                    && map.get("index").is_none()
                {
                    if let Some(vnum) = map.get("value").and_then(|v| v.as_u64()) {
                        map.insert("index".into(), Value::Number(vnum.into()));
                    }
                }
                if op_str.as_str() == "RETURN" {
                    if let Some(v) = map.get("value") {
                        if v.is_number() {
                            if let Some(fv) = v.as_f64() {
                                map.insert(
                                    "value".into(),
                                    serde_json::json!({"op":"F64_CONST","value":fv}),
                                );
                            }
                        }
                    }
                }
                if (op_str.as_str() == "BLOCK" || op_str.as_str() == "LOOP")
                    && map.get("body").is_none()
                {
                    if let Some(children) = map.remove("children") {
                        map.insert("body".into(), children);
                    }
                }
            }
            if let Some(op) = map.get("op").and_then(|v| v.as_str()) {
                if (op == "GET_LOCAL" || op == "SET_LOCAL") && map.get("index").is_some() {
                    if let Some(idx) = map.get("index").and_then(|v| v.as_u64()) {
                        let resolved = resolve_ir_index(idx as usize, params, locals);
                        if op == "GET_LOCAL" {
                            map.insert("name".into(), Value::String(resolved));
                        } else if op == "SET_LOCAL" {
                            map.insert("local".into(), Value::String(resolved));
                        }
                        map.remove("index");
                    }
                }
            }
            for key in ["body", "then", "else_", "children"] {
                if let Some(child) = map.get_mut(key) {
                    normalize_ir_indices(child, params, locals);
                }
            }
        }
        _ => {}
    }
}

fn resolve_ir_index(idx: usize, params: Option<&Value>, locals: Option<&Value>) -> String {
    let mut names: Vec<String> = Vec::new();
    if let Some(pa) = params.and_then(|v| v.as_array()) {
        for p in pa {
            if let Some(n) = p.get("name").and_then(|v| v.as_str()) {
                names.push(n.to_string());
            }
        }
    }
    if let Some(la) = locals.and_then(|v| v.as_array()) {
        for l in la {
            if let Some(n) = l.get("name").and_then(|v| v.as_str()) {
                names.push(n.to_string());
            }
        }
    }
    if idx < names.len() {
        names[idx].clone()
    } else {
        format!("idx{idx}")
    }
}

fn generic_ir_quality(ir: &IRFunction) -> bool {
    let mut loops = 0usize;
    let mut arith = 0usize;
    let mut param_reads = 0usize;
    let mut non_const = false;
    fn walk(
        node: &IRNode,
        loops: &mut usize,
        arith: &mut usize,
        param_reads: &mut usize,
        non_const: &mut bool,
    ) {
        match node {
            IRNode::LOOP { body, .. } => {
                *loops += 1;
                for n in body {
                    walk(n, loops, arith, param_reads, non_const);
                }
            }
            IRNode::I64_ADD { left, right }
            | IRNode::I64_SUB { left, right }
            | IRNode::I64_MUL { left, right }
            | IRNode::I64_REM_S { left, right } => {
                walk(left, loops, arith, param_reads, non_const);
                walk(right, loops, arith, param_reads, non_const);
                *arith += 1;
                *non_const = true;
            }
            IRNode::GET_LOCAL { .. } => {
                *param_reads += 1;
                *non_const = true;
            }
            IRNode::SET_LOCAL { expr, .. } => {
                if let Some(e) = expr {
                    walk(e, loops, arith, param_reads, non_const);
                }
                *non_const = true;
            }
            IRNode::BLOCK { body, .. } => {
                for n in body {
                    walk(n, loops, arith, param_reads, non_const);
                }
            }
            IRNode::RETURN { value } => {
                walk(value, loops, arith, param_reads, non_const);
            }
            IRNode::IF { cond, then, else_ } => {
                walk(cond, loops, arith, param_reads, non_const);
                for n in then {
                    walk(n, loops, arith, param_reads, non_const);
                }
                for n in else_ {
                    walk(n, loops, arith, param_reads, non_const);
                }
                *non_const = true;
            }
            _ => {}
        }
    }
    for n in &ir.body {
        walk(n, &mut loops, &mut arith, &mut param_reads, &mut non_const);
    }

    (loops > 0 || arith >= 2) && non_const && (param_reads > 0 || loops > 0)
}
