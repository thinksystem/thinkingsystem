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
use stele::codegen::artifacts::{persist_wat_artifact, persist_wat_sidecar, WatMetadata};
use stele::flows::dynamic_executor::DynamicSource;
use stele::nlu::llm_processor::LLMAdapter;
use tracing::{info, warn};

pub async fn sanitize_register(
    engine: &mut stele::UnifiedFlowEngine,
    adapter: &dyn LLMAdapter,
    directive: &str,
    name: &str,
    export: &str,
    wat_in: &str,
    artifacts_dir: &str,
    offline: bool,
    max_wat_repairs: u8,
) -> Result<String> {
    sanitize_register_dual(
        engine,
        adapter,
        adapter,
        directive,
        name,
        export,
        wat_in,
        artifacts_dir,
        offline,
        max_wat_repairs,
    )
    .await
}

pub async fn sanitize_register_dual(
    engine: &mut stele::UnifiedFlowEngine,
    primary_adapter: &dyn LLMAdapter,
    supervisor_adapter: &dyn LLMAdapter,
    directive: &str,
    name: &str,
    export: &str,
    wat_in: &str,
    artifacts_dir: &str,
    offline: bool,
    max_wat_repairs: u8,
) -> Result<String> {
    let _ = persist_wat_artifact(artifacts_dir, name, wat_in, "raw_initial");
    let mut current = wat_in.to_string();

    fn assemble_and_canonicalize(src: &str) -> Result<(Vec<u8>, String)> {
        let bytes = wat::parse_str(src).map_err(|e| anyhow::anyhow!("assemble_error: {e}"))?;
        let canon =
            wasmprinter::print_bytes(&bytes).map_err(|e| anyhow::anyhow!("print_error: {e}"))?;
        Ok((bytes, canon))
    }

    fn fix_wat_strings(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut in_str = false;
        let mut escaped = false;
        let mut iter = input.chars().peekable();
        while let Some(ch) = iter.next() {
            if in_str {
                if escaped {
                    match ch {
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        'n' | 'r' | 't' => {
                            out.push('\\');
                            out.push(ch);
                        }
                        _ => {
                            out.push('\\');
                            out.push(ch);
                        }
                    }
                    escaped = false;
                } else {
                    match ch {
                        '"' => {
                            in_str = false;
                            out.push('"');
                        }
                        '\\' => {
                            escaped = true;
                        }
                        '\n' | '\r' => {}
                        _ => out.push(ch),
                    }
                }
            } else {
                match ch {
                    '"' => {
                        in_str = true;
                        out.push('"');
                    }
                    '\\' => {
                        if let Some('"') = iter.peek().copied() {
                            let _ = iter.next();
                            in_str = true;
                            out.push('"');
                        } else {
                            out.push('\\');
                        }
                    }
                    _ => out.push(ch),
                }
            }
        }
        out
    }

    fn normalize_if_stack_form(input: &str) -> Option<String> {
        let bytes = input.as_bytes();
        let mut i = 0usize;
        let mut out = String::with_capacity(input.len());
        while i < bytes.len() {
            if bytes[i] == b'(' && bytes.get(i + 1) == Some(&b'i') && input[i..].starts_with("(if ")
            {
                let mut j = i + 4;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'(' {
                    let cond_start = j;
                    let mut depth = 0i32;
                    let mut k = j;
                    let mut found_end = None;
                    while k < bytes.len() {
                        let ch = bytes[k] as char;
                        if ch == '(' {
                            depth += 1;
                        } else if ch == ')' {
                            depth -= 1;
                            if depth == 0 {
                                found_end = Some(k);
                                break;
                            }
                        }
                        k += 1;
                    }
                    if let Some(cond_end) = found_end {
                        let cond_expr = &input[cond_start..=cond_end];
                        let rest_after_cond = &input[cond_end + 1..];
                        out.push_str(&input[..i]);
                        out.push_str(cond_expr);
                        out.push(' ');
                        out.push_str("(if");
                        out.push_str(rest_after_cond);
                        return Some(out);
                    }
                }
            }
            i += 1;
        }
        None
    }

    fn wrap_then_else_blocks(input: &str) -> Option<String> {
        let bytes = input.as_bytes();
        let mut i = 0usize;
        let mut changed = false;
        let mut out = String::with_capacity(input.len() + 32);
        while i < bytes.len() {
            if bytes[i] == b'(' && input[i..].starts_with("(if") {
                let if_start = i;

                i += 3;

                let mut depth: i32 = 1;
                let mut j = i;

                while j < bytes.len() && depth > 0 {
                    let ch = bytes[j] as char;
                    if ch == '(' {
                        depth += 1;
                        j += 1;
                        continue;
                    } else if ch == ')' {
                        depth -= 1;
                        j += 1;
                        continue;
                    }
                    j += 1;
                }
                let if_end = j;

                let inner = &input[if_start..if_end];

                if inner.contains("(then") || inner.contains("(else") {
                    out.push_str(inner);
                } else if inner.contains(" then ")
                    || inner.contains("\nthen ")
                    || inner.contains(" else ")
                    || inner.contains("\nelse ")
                {
                    let after_if = 3usize;
                    out.push_str(&inner[..after_if]);

                    let rest = &inner[after_if..];
                    let lower = rest;
                    let t_pos = lower.find(" then ").or_else(|| lower.find("\nthen "));
                    let e_pos = lower.find(" else ").or_else(|| lower.find("\nelse "));
                    if let Some(t_off) = t_pos {
                        out.push_str(&rest[..t_off]);
                        out.push_str(" (then ");

                        let then_body_start = t_off + 6;
                        let then_body_end = e_pos.unwrap_or(rest.len());
                        out.push_str(&rest[then_body_start..then_body_end]);
                        out.push(')');
                        if let Some(e_off) = e_pos {
                            out.push_str(" (else ");
                            let else_body_start = e_off + 6;
                            out.push_str(&rest[else_body_start..]);
                            out.push(')');
                        }
                        changed = true;
                    } else if let Some(e_off) = e_pos {
                        out.push_str(" (then ");
                        let else_body_start = e_off + 6;
                        out.push_str(&rest[..e_off]);
                        out.push(')');
                        out.push_str(" (else ");
                        out.push_str(&rest[else_body_start..]);
                        out.push(')');
                        changed = true;
                    } else {
                        out.push_str(rest);
                    }
                } else {
                    out.push_str(inner);
                }

                i = if_end;
            } else {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
        if changed {
            Some(out)
        } else {
            None
        }
    }

    fn add_memory_if_needed(input: &str) -> Option<String> {
        let has_mem_ops = input.contains(".load") || input.contains(".store");
        let has_memory = input.contains("(memory");
        if has_mem_ops && !has_memory {
            let replaced = input.replacen("(module", "(module\n  (memory 1)", 1);
            return Some(replaced);
        }
        None
    }

    fn ensure_export_name(canonical: &str, export: &str) -> String {
        use regex::Regex;
        let out = canonical.to_string();

        let re_top = Regex::new(r#"\(export\s+"[^"]+"\s+\(func\b"#).ok();
        if let Some(re) = re_top.as_ref() {
            let replaced = re
                .replace(out.as_str(), format!("(export \"{export}\" (func"))
                .to_string();
            if replaced != out {
                return replaced.to_string();
            }
        }

        let re_inline = Regex::new(r#"\(func([^\)]*)\(export\s+"[^"]+"\)"#).ok();
        if let Some(re) = re_inline.as_ref() {
            let replaced = re
                .replace(out.as_str(), |caps: &regex::Captures| {
                    let pre = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                    format!("(func{pre}(export \"{export}\")")
                })
                .to_string();
            if replaced != out {
                return replaced;
            }
        }
        out
    }

    let mut canonical: String;
    let mut llm_repairs: u8 = 0;

    fn contains_obviously_invalid_tokens(input: &str) -> bool {
        input.contains('*')
            || input.contains('[')
            || input.contains(']')
            || input.contains(" .. ")
            || input.contains(".. ")
            || input.contains(" ..")
            || input.contains("$[")
            || input.contains("->$")
    }

    loop {
        match assemble_and_canonicalize(&current) {
            Ok((_, c)) => {
                canonical = ensure_export_name(&c, export);
                break;
            }
            Err(e) => {
                let fixed = fix_wat_strings(&current);
                if fixed != current {
                    let _ = persist_wat_artifact(artifacts_dir, name, &fixed, "string_fix");
                    current = fixed;
                    continue;
                }

                if let Some(nfixed) = normalize_if_stack_form(&current) {
                    let _ = persist_wat_artifact(artifacts_dir, name, &nfixed, "if_stack_fix");
                    current = nfixed;
                    continue;
                }
                if let Some(tfixed) = wrap_then_else_blocks(&current) {
                    let _ = persist_wat_artifact(artifacts_dir, name, &tfixed, "then_else_wrapped");
                    current = tfixed;
                    continue;
                }
                if let Some(mfixed) = add_memory_if_needed(&current) {
                    let _ = persist_wat_artifact(artifacts_dir, name, &mfixed, "memory_injected");
                    current = mfixed;
                    continue;
                }

                if offline || llm_repairs >= max_wat_repairs {
                    return Err(e);
                }
                let mut es = e.to_string();

                let mut guidance = String::new();

                guidance.push_str("Rewrite the entire module as valid WebAssembly Text (WAT) version 1.0 with this canonical shape: (module (func $NAME (export \"EXPORT\") (param $n i32) (result i32) ...body...)). Rules: 1) Declare all locals at the top with explicit types, e.g., (local $i i32) (local $sqrt i32). 2) Every (if ...) must include both (then ...) and (else ...); do not omit else. If no else action, use (else (nop)). 3) IMPORTANT: The condition must be evaluated before `if`. Emit `(COND) (if (then ...) (else ...))`, NOT `(if (COND) (then ...))`. 4) For loops, use (block (loop ... (br_if 1 (COND)) ... (br 0))) to exit or continue; br_if takes a depth immediate number, not a then/else form. 5) Use only valid opcodes: i32.rem_u, i32.eq, i32.lt_u, i32.gt_u, i32.add, i32.const, local.get, local.set, block, loop, br, br_if, return. 6) Do not write pseudo-code like \"$i = 0\"; instead use (local.set $i (i32.const 0)). 7) Do not attach labels to expressions; labels follow block/loop/if. 8) Do not escape export names; use plain quotes. 9) Ensure parentheses are balanced and there are no stray instructions after returns. 10) Keep a single function export exactly named EXPORT. 11) Do not include comments or markdown, only the pure WAT. ");
                guidance.push_str("Strict bans: do NOT use '*' for multiplication (use i32.mul/i64.mul), do NOT use array-like indexing such as $arr[i] or square brackets, do NOT use '..' range syntax, do NOT use '+', '-' inline between identifiers; always use proper opcodes like i32.add, i32.sub with local.get/local.set. Keep to locals only; no pseudo arrays.");

                if contains_obviously_invalid_tokens(&current) {
                    guidance.push_str(" The previous snippet contained invalid tokens (e.g., '*', '[]', or '..' range). Replace them with valid opcodes and locals-only logic.");
                }
                if es.contains("expected keyword `else`") {
                    guidance.push_str("Hint: WebAssembly textual 'if' must be of the form (if <cond> (then ...) (else ...)). If there is nothing to do in else, emit (else) with a no-op like (nop) or move the condition into a br_if. Ensure parentheses are balanced. ");
                }
                if es.contains("unknown operator or unexpected token") {
                    guidance.push_str("Hint: Use only valid opcodes and syntax. Do not include stray tokens. Export strings should be quoted with plain double quotes, not escaped. ");
                }
                if !guidance.is_empty() {
                    es = format!("{es} | {guidance}");
                }

                let (first, second) = if llm_repairs % 2 == 0 {
                    (primary_adapter, supervisor_adapter)
                } else {
                    (supervisor_adapter, primary_adapter)
                };
                let mut repaired_res = stele::codegen::wat_pipeline::attempt_wat_repair(
                    first, directive, name, export, &current, &es,
                )
                .await;
                if repaired_res.is_err() {
                    repaired_res = stele::codegen::wat_pipeline::attempt_wat_repair(
                        second, directive, name, export, &current, &es,
                    )
                    .await;
                }
                match repaired_res {
                    Ok(w) => {
                        let _ = persist_wat_artifact(artifacts_dir, name, &w, "repair_attempt_llm");
                        current = w;
                        llm_repairs = llm_repairs.saturating_add(1);
                        continue;
                    }
                    Err(re) => {
                        return Err(anyhow::anyhow!(
                            "WAT assembly failed and repair unsuccessful: {es} / {re}"
                        ));
                    }
                }
            }
        }
    }

    let (_unused, metrics) = stele::codegen::wat_pipeline::sanitize_with_metrics(&canonical);
    let path = persist_wat_artifact(artifacts_dir, name, &canonical, "final")?;
    let exists = std::path::Path::new(&path).exists();
    info!(function=%name, path=%path, exists, loops=%metrics.loops, arithmetic_ops=%metrics.arithmetic_ops, param_reads=%metrics.param_reads, len=%metrics.length_bytes, "final WAT persisted");
    let meta = WatMetadata {
        function: name,
        suffix: "final",
        passes_applied: vec![],
        loops: metrics.loops,
        arithmetic_ops: metrics.arithmetic_ops,
        param_reads: metrics.param_reads,
        length_bytes: metrics.length_bytes,
        rng_canonicalized: false,
    };
    let _ = persist_wat_sidecar(&path, &meta);

    let reg_attempts = if offline || max_wat_repairs == 0 {
        1
    } else {
        2
    };
    for attempt in 1..=reg_attempts {
        match engine.register_dynamic_source(
            name,
            DynamicSource::Wat {
                name,
                export,
                wat: &canonical,
            },
        ) {
            Ok(_) => {
                info!(
                    stage = "FUNCTION_SUMMARY",
                    function = %name,
                    attempts = attempt,
                    loops = metrics.loops,
                    arithmetic_ops = metrics.arithmetic_ops,
                    param_reads = metrics.param_reads,
                    len = metrics.length_bytes,
                    "Function accepted with canonical WAT"
                );
                return Ok(canonical);
            }
            Err(e) => {
                if attempt == reg_attempts {
                    warn!(function=%name, error=%e, "Registration failed after repairs; proceeding without registration");

                    return Ok(canonical);
                }
                let es = e.to_string();

                warn!(function=%name, error=%es, "registration failed; attempting LLM repair and re-assemble");
                if offline {
                    warn!(function=%name, "Offline mode prevents LLM repair; proceeding without registration");
                    return Ok(canonical);
                }
                let mut repaired_res = stele::codegen::wat_pipeline::attempt_wat_repair(
                    primary_adapter,
                    directive,
                    name,
                    export,
                    &canonical,
                    &es,
                )
                .await;
                if repaired_res.is_err() {
                    repaired_res = stele::codegen::wat_pipeline::attempt_wat_repair(
                        supervisor_adapter,
                        directive,
                        name,
                        export,
                        &canonical,
                        &es,
                    )
                    .await;
                }
                let repaired = repaired_res?;
                let _ = persist_wat_artifact(
                    artifacts_dir,
                    name,
                    &repaired,
                    "repair_attempt_register_dual",
                );
                match assemble_and_canonicalize(&repaired) {
                    Ok((_, c)) => {
                        canonical = ensure_export_name(&c, export);
                    }
                    Err(ee) => {
                        warn!(function=%name, error=%ee, "Post-repair assembly failed; keeping previous canonical");
                        return Ok(canonical);
                    }
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "register loop exited unexpectedly without returning"
    ))
}
