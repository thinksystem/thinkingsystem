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


use crate::blocks::rules::BlockError;
use crate::codegen::{
    artifacts::{persist_wat_artifact, persist_wat_sidecar, WatMetadata},
    wat_sanitize,
};


pub fn sanitize_with_metrics(raw: &str) -> (String, wat_sanitize::WatStructuralMetrics) {
    wat_sanitize::sanitize_wat_basic(raw)
}


pub fn register_wat_with_artifacts<F>(
    artifacts_dir: &str,
    name: &str,
    export: &str,
    raw_wat: &str,
    mut compile: F,
) -> Result<(), BlockError>
where
    F: FnMut(&str, &str) -> Result<(), BlockError>,
{
    let (cleaned, metrics) = sanitize_with_metrics(raw_wat);
    let path = persist_wat_artifact(artifacts_dir, name, &cleaned, "sanitized")
        .map_err(|e| BlockError::ProcessingError(e.to_string()))?;
    let meta = WatMetadata {
        function: name,
        suffix: "sanitized",
        passes_applied: vec![],
        loops: metrics.loops,
        arithmetic_ops: metrics.arithmetic_ops,
        param_reads: metrics.param_reads,
        length_bytes: metrics.length_bytes,
        rng_canonicalized: false,
    };
    let _ = persist_wat_sidecar(&path, &meta);
    compile(&cleaned, export)?;
    Ok(())
}


#[derive(Debug)]
pub struct StructuralVerdict {
    pub loops: u32,
    pub arithmetic_ops: u32,
    pub param_reads: u32,
    pub acceptable: bool,
}

pub fn structural_vetting(wat: &str) -> StructuralVerdict {
    let m = wat_sanitize::sanitize_wat_basic(wat).1; 
    let acceptable = m.loops > 0 && m.arithmetic_ops >= 2; 
    StructuralVerdict {
        loops: m.loops,
        arithmetic_ops: m.arithmetic_ops,
        param_reads: m.param_reads,
        acceptable,
    }
}


use crate::nlu::llm_processor::LLMAdapter;

fn normalize_wat_candidate(input: &str) -> String {
    let mut s = input.trim().to_string();
    
    if s.starts_with("```") {
        
        if let Some(pos) = s.find('\n') {
            s = s[pos + 1..].to_string();
        }
    }
    if s.ends_with("```") {
        if let Some(pos) = s.rfind("```") {
            s = s[..pos].to_string();
        }
    }
    
    if s.starts_with('{') && s.contains("\"wat\"") {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&s) {
            if let Some(inner) = val.get("wat").and_then(|v| v.as_str()) {
                return inner.to_string();
            }
        }
    }
    
    if s.contains("\\n") && !s.contains('\n') {
        s = s.replace("\\n", "\n").replace("\\t", "\t");
    }
    
    
    if s.starts_with("(module") && s.contains("\\\"") {
        s = s.replace("\\\"", "\"");
    }
    
    if s.contains("\\(") {
        s = s.replace("\\(", "(");
    }
    
    
    
    if s.contains("get_local ") || s.contains("set_local ") || s.contains("tee_local ") {
        s = s
            .replace("(get_local ", "(local.get ")
            .replace("(set_local ", "(local.set ")
            .replace("(tee_local ", "(local.tee ");
    }
    
    if s.starts_with('"') && s.ends_with('"') && s.matches('"').count() == 2 {
        s = s.trim_matches('"').to_string();
    }
    
    if let Some(idx) = s.find("(module") {
        if idx > 0 {
            s = s[idx..].to_string();
        }
    }
    
    if let Some(last) = s.rfind(')') {
        let trailing = &s[last + 1..];
        if trailing.trim_start().starts_with('"') || trailing.contains('}') {
            s = s[..=last].to_string();
        }
    }
    s.trim().to_string()
}


fn fix_nested_function_defs(candidate: &str) -> String {
    
    if candidate.match_indices("(func $").count() < 2 {
        return candidate.to_string();
    }
    let mut out = String::with_capacity(candidate.len() + 8);
    let mut balance: i32 = 0; 
    let mut seen_first_func = false;
    for line in candidate.lines() {
        let pre_balance = balance;
        
        let trimmed = line.trim_start();
        let is_func_line = trimmed.starts_with("(func $");
        if is_func_line {
            if !seen_first_func {
                seen_first_func = true;
            } else if pre_balance > 2 {
                
                out.push_str(")\n");
                balance -= 1; 
            }
        }
        out.push_str(line);
        out.push('\n');
        
        for ch in line.chars() {
            if ch == '(' {
                balance += 1;
            } else if ch == ')' {
                balance -= 1;
            }
        }
    }
    out
}


pub async fn attempt_wat_repair(
    adapter: &dyn LLMAdapter,
    directive: &str,
    name: &str,
    export: &str,
    previous_wat: &str,
    error: &str,
) -> anyhow::Result<String> {
    let system = r#"You repair WebAssembly Text (WAT). Output ONLY JSON {\"wat\":\"(module ... )\"}.
STRICT RULES:
1. Provide exactly one (module ...) root.
2. Provide one (func $NAME (param ...) (result ...)? ...) where NAME is the given function name.
3. Export that function with the provided export label EXACTLY ( (export \"<export>\" (func $NAME)) ). If export label differs from NAME, still use the provided label only once.
4. No extra commentary, no markdown fences, no backticks, no surrounding quotes outside JSON encoding.
5. No imports, no calls to undefined functions, no host intrinsics.
6. Preserve and use consistent numeric types across the function (i32/i64/f32/f64) matching the existing snippet when possible; do not mix widths within the same operation.
7. Avoid returning a hard constant unrelated to directive; implement minimal plausible numeric logic / loop only if needed.
8. Keep module minimal: just the func + exports (and internal locals / loop / math). No debug names.
Return JSON ONLY."#;
    let user = format!(
        "Directive: {directive}\nFunctionName: {name}\nExportLabel: {export}\nPreviousCandidate:\n{previous_wat}\nErrorContext: {error}\nCommon mistakes to avoid: wrapping JSON again, code fences, missing export, wrong func name, extra imports. Re-emit strictly. Return JSON now."
    );
    let resp = adapter
        .generate_structured_response(system, &user)
        .await
        .map_err(|e| anyhow::anyhow!("LLM repair call failed: {e}"))?;
    if let Some(w) = resp.get("wat").and_then(|v| v.as_str()) {
        return Ok(normalize_wat_candidate(w));
    }
    for (_k, v) in resp.as_object().into_iter().flat_map(|m| m.iter()) {
        if let Some(s) = v.as_str() {
            if s.contains("(module") {
                return Ok(normalize_wat_candidate(s));
            }
        }
    }
    Err(anyhow::anyhow!("LLM did not return repaired WAT"))
}


pub async fn sanitize_validate_with_llm(
    adapter: &dyn LLMAdapter,
    directive: &str,
    name: &str,
    export: &str,
    initial: &str,
    max_attempts: u8,
) -> anyhow::Result<String> {
    let mut current = initial.to_string();
    
    let mut last_err_sig: Option<String> = None;
    let mut repeat_err_count: u8 = 0;
    for attempt in 1..=max_attempts {
        let mut cleaned = normalize_wat_candidate(&current);
        cleaned = fix_nested_function_defs(&cleaned);
        
        if cleaned.contains("(result i64") && !cleaned.contains("(if (result i64)") {
            
            let tail_window = cleaned
                .split('\n')
                .rev()
                .take(8)
                .collect::<Vec<_>>()
                .join("\n");
            let pred_tokens = [
                "(i64.eq",
                "(i64.ne",
                "(i64.lt_s",
                "(i64.lt_u",
                "(i64.gt_s",
                "(i64.gt_u",
                "(i64.le_s",
                "(i64.le_u",
                "(i64.ge_s",
                "(i64.ge_u",
            ];
            if pred_tokens.iter().any(|p| tail_window.contains(p)) {
                
                if let Some(pos) = cleaned.trim_end().rfind(')') {
                    let (head, tail) = cleaned.split_at(pos);
                    if !head
                        .ends_with("(if (result i64) (then (i64.const 1)) (else (i64.const 0)))")
                    {
                        cleaned = format!("{head}\n  (if (result i64) (then (i64.const 1)) (else (i64.const 0)))\n{tail}");
                    }
                }
            }
        }
        
        let mut issues: Vec<&str> = Vec::new();
        if !cleaned.starts_with("(module") {
            issues.push("missing_module_prefix");
        }
        if cleaned.matches("(module").count() > 1 {
            issues.push("multiple_module_tokens");
        }
        if cleaned.contains("(import") {
            issues.push("imports_present");
        }
        if !cleaned.contains(&format!("(func ${name}")) {
            issues.push("missing_named_func");
        }
        if !cleaned.contains(&format!("export \"{export}\"")) {
            issues.push("missing_export_label");
        }
        
        let open_parens = cleaned.matches('(').count();
        let close_parens = cleaned.matches(')').count();
        if open_parens > close_parens {
            issues.push("unbalanced_parens");
        }
        if open_parens > close_parens {
            let mut tmp = cleaned.clone();
            for _ in 0..(open_parens - close_parens) {
                tmp.push(')');
            }
            cleaned = tmp;
        }
        match wat::parse_str(&cleaned) {
            Ok(_) => {
                
                let missing_func = !cleaned.contains(&format!("(func ${name}"));
                let missing_export = !cleaned.contains(&format!("export \"{export}\""));
                if (!issues.is_empty() && (missing_func || missing_export))
                    && attempt < max_attempts
                {
                    let err_ctx = if missing_export {
                        format!("structural_issues:{issues:?};expected_export_line:(export \"{export}\" (func ${name}))")
                    } else {
                        format!("structural_issues:{issues:?}")
                    };
                    current =
                        attempt_wat_repair(adapter, directive, name, export, &cleaned, &err_ctx)
                            .await?;
                    continue;
                }
                return Ok(cleaned);
            }
            Err(e) if attempt < max_attempts => {
                
                let err_s = e.to_string();
                if let Some(prev) = &last_err_sig {
                    if prev == &err_s {
                        repeat_err_count += 1;
                    } else {
                        repeat_err_count = 1;
                    }
                } else {
                    repeat_err_count = 1;
                }
                last_err_sig = Some(err_s.clone());
                if repeat_err_count >= 3 {
                    return Err(anyhow::anyhow!(format!("repeated_parse_error:{err_s}")));
                }
                let missing_export = issues.contains(&"missing_export_label");
                let err_ctx = if missing_export {
                    format!("parse_error:{e};issues:{issues:?};expected_export_line:(export \"{export}\" (func ${name}))")
                } else {
                    format!("parse_error:{e};issues:{issues:?}")
                };
                current = attempt_wat_repair(adapter, directive, name, export, &cleaned, &err_ctx)
                    .await?;
            }
            Err(e) => {
                return Err(anyhow::anyhow!(format!(
                    "validation failed after attempts: {e}; issues:{:?}",
                    issues
                )));
            }
        }
    }
    unreachable!("loop ends via return or error")
}
