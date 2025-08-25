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
use stele::nlu::llm_processor::LLMAdapter;

use super::artifact::RustFnArtifact;
use super::sanitize::{try_clean_code, wrap_body_as_compute};
#[cfg(all(feature = "dynamic-wasi", not(feature = "dynamic-native")))]
use crate::rust_fn::healing::{
    enforce_exponent_intent, enforce_reciprocal_intent, post_sanitize_code,
};

pub async fn generate_rust_source(
    adapter: &dyn LLMAdapter,
    directive: &str,
) -> Result<RustFnArtifact> {
    
    let system = r#"You output ONLY JSON with keys:
{"code":string,"args": [number,...],"magnitude":[low,high]}
CODE requirements:
- Minimal safe Rust for cdylib with exactly exported function:
    #[no_mangle] pub extern "C" fn compute(inputs_ptr:*const u8,len:usize,out_ptr:*mut u8,out_len:usize)->i32
- Parse inputs_ptr/len as UTF-8 JSON array of numbers (match provided args count).
- Perform ONLY pure arithmetic implied by directive.
- No file/net/env/process/threading; no external crates; no macros except simple derives; no randomness.
- Use f64; numeric JSON result (single number) representing directive answer.
- Do NOT use integer accumulator generics like sum::<u32>() / sum::<i32>(); always accumulate as f64 (sum::<f64>() or let inference pick f64) and prefer mapping inputs to f64 early.
- Do NOT emit a nested or duplicate compute function inside another; emit exactly one compute function and no embedded JSON fragments inside code.
- Write UTF-8 JSON bytes of result into out_ptr (<= out_len-1) then null terminate; return 0 success else nonzero.
- Keep code under 80 lines.
- After the compute function closing brace add a single line comment of the form:
    // RESULT: <variable_name>
    where <variable_name> is the identifier holding the final numeric answer BEFORE serialization (do NOT fabricate if unnecessary; if you already named it 'result' keep it consistent). This comment is mandatory.
ARGS:
- Provide concrete numeric test args (array) the function expects; empty array if none required.
MAGNITUDE:
- Provide loose expected numeric magnitude [low, high] for the final result (broad, order-of-magnitude, not tight). If unknown use [-1e308,1e308].
Return ONLY JSON (no markdown)."#;
    let user = format!("Directive: {directive}\nReturn JSON now.");
    let mut last: Option<anyhow::Error> = None;
    let max_attempts: u8 = 10; 
    let mut attempt: u8 = 0;
    let mut feedback: Option<String> = None;
    while attempt < max_attempts {
        attempt += 1;
        
        let user_adaptive = if let Some(fb) = &feedback {
            format!("Directive: {directive}\nPrevious attempt reason: {fb}\nRegenerate fresh code now. Return JSON only.")
        } else {
            user.clone()
        };
        let resp = match adapter
            .generate_structured_response(system, &user_adaptive)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last = Some(anyhow::anyhow!("llm error: {e}"));
                feedback = Some(format!("llm_transport_or_parse_error: {e}"));
                continue;
            }
        };
        let args_vec: Vec<f64> = resp
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|n| n.as_f64()).collect())
            .unwrap_or_default();
        let magnitude = resp
            .get("magnitude")
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                if arr.len() == 2 {
                    if let (Some(a), Some(b)) = (arr[0].as_f64(), arr[1].as_f64()) {
                        Some((a, b))
                    } else {
                        None
                    }
                } else {
                    None
                }
            });
        if let Some(c_raw) = resp.get("code").and_then(|v| v.as_str()) {
            if !(c_raw.contains("pub extern \"C\" fn compute")
                || c_raw.contains("pub extern \\\"C\\\" fn compute"))
            {
                
                feedback = Some("missing_compute_signature".to_string());
                last = Some(anyhow::anyhow!("missing compute signature"));
                continue;
            }
            if let Some(mut clean) = try_clean_code(c_raw) {
                
                use crate::cleaning::native::{clean as passes_clean, NativeContext, WasmMode};
                #[allow(unused)]
                fn detect_mode() -> WasmMode {
                    #[cfg(all(feature = "dynamic-wasi", not(feature = "dynamic-native")))]
                    { WasmMode::Wasi }
                    #[cfg(all(feature = "dynamic-native", not(feature = "dynamic-wasi")))]
                    { WasmMode::Native }
                    #[cfg(all(feature = "dynamic-native", feature = "dynamic-wasi"))]
                    { WasmMode::Native }
                    #[cfg(all(not(feature = "dynamic-native"), not(feature = "dynamic-wasi")))]
                    { compile_error!("At least one of dynamic-native or dynamic-wasi must be enabled"); }
                }
                if let Some(pc) = passes_clean(&clean, &NativeContext { directive: directive.to_string(), wasm_mode: detect_mode() }) {
                    clean = pc;
                }
                
                if clean.matches("pub extern \"C\" fn compute").count() > 1
                    || clean.contains("let result: f64 = { {")
                {
                    last = Some(anyhow::anyhow!("nested compute or embedded JSON artifact"));
                    feedback = Some("nested_or_embedded_artifact".to_string());
                    continue;
                }
                #[cfg(all(feature = "dynamic-wasi", not(feature = "dynamic-native")))]
                {
                    
                    clean = post_sanitize_code(&clean);
                    clean = enforce_exponent_intent(&clean, directive);
                    clean = enforce_reciprocal_intent(&clean, directive);
                }
                if !clean.contains("pub extern \"C\" fn compute") {
                    clean = wrap_body_as_compute(&clean);
                }
                
                if clean.contains("copy_nonoverlap(") {
                    clean = clean.replace("copy_nonoverlap(", "copy_nonoverlapping(");
                }
                
                
                
                if clean.contains("\n    0\n    return 0;") {
                    clean = clean.replace("\n    0\n    return 0;", "\n    return 0;");
                }
                if clean.contains("\n0\nreturn 0;") {
                    clean = clean.replace("\n0\nreturn 0;", "\nreturn 0;");
                }
                
                
                if let Some(contract_pos) = clean.rfind("// RESULT:") {
                    let contract_line = clean[contract_pos..]
                        .lines()
                        .next()
                        .unwrap_or("")
                        .to_string();
                    let var = contract_line
                        .split(':')
                        .nth(1)
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default();
                    if !var.is_empty() {
                        let needle1 = "let result: f64 = result as f64;";
                        if clean.contains(needle1) {
                            if var == "result" {
                                
                                clean = clean.replace(needle1, "");
                            } else {
                                clean = clean
                                    .replace(needle1, &format!("let result: f64 = {var} as f64;"));
                            }
                        }
                        let needle2 = "let result = result as f64;";
                        if clean.contains(needle2) {
                            if var == "result" {
                                clean = clean.replace(needle2, "");
                            } else {
                                clean =
                                    clean.replace(needle2, &format!("let result = {var} as f64;"));
                            }
                        }
                        let needle3 = "let result: f64 = result;";
                        if clean.contains(needle3) {
                            if var == "result" {
                                clean = clean.replace(needle3, "");
                            } else {
                                clean = clean
                                    .replace(needle3, &format!("let result: f64 = {var} as f64;"));
                            }
                        }
                        
                        for pat in [
                            "let  result : f64 = result as f64;",
                            "let result : f64 = result as f64;",
                            "let result: f64 =  result as f64;",
                        ]
                        .iter()
                        {
                            if var == "result" && clean.contains(pat) {
                                clean = clean.replace(pat, "");
                            }
                        }
                    }
                }
                
                
                if clean.contains("result.to_string()")
                    && clean.contains("serde_json::to_string(&result)")
                {
                    if let Some(start_idx) = clean.find("let result_str") {
                        
                        let marker_candidates = [
                            "let json = match serde_json::to_string(&result)",
                            "let json = serde_json::to_string(&result)",
                            "match serde_json::to_string(&result)",
                        ];
                        let mut next_idx: Option<usize> = None;
                        for m in marker_candidates.iter() {
                            if let Some(pos) = clean[start_idx + 20..].find(m) {
                                
                                next_idx = Some(start_idx + 20 + pos);
                                break;
                            }
                        }
                        if next_idx.is_none() {
                            
                            if let Some(pos2) =
                                clean[start_idx + 20..].find("let result: f64 = result as f64;")
                            {
                                next_idx = Some(start_idx + 20 + pos2);
                            }
                        }
                        if let Some(end_idx) = next_idx {
                            
                            let to_remove = &clean[start_idx..end_idx];
                            
                            if to_remove.contains("copy_nonoverlapping") {
                                clean = clean.replacen(to_remove, "", 1);
                            }
                        }
                    }
                }
                return Ok(RustFnArtifact {
                    code: clean,
                    args: args_vec,
                    magnitude,
                    system_prompt: system.to_string(),
                });
            } else {
                last = Some(anyhow::anyhow!("code field failed cleaning"));
                feedback = Some("cleaning_failed".to_string());
                continue;
            }
        }
        
    if resp.as_object().map(|o| o.len() == 1).unwrap_or(false) {
            if let Some((_k, v)) = resp.as_object().and_then(|o| o.iter().next()) {
                if let Some(s_raw) = v.as_str() {
                    
                    let mut attempt_src: Option<String> = try_clean_code(s_raw);
                    if attempt_src.is_none() {
                        
                        if let Some(sig_pos) = s_raw.find("pub extern \"C\" fn compute") {
                            let slice = &s_raw[sig_pos..];
                            
                            if let Some(brace_rel) = slice.find('{') {
                                let mut depth: i32 = 0;
                                let mut end_idx: Option<usize> = None;
                                for (i, ch) in slice[brace_rel..].char_indices() {
                                    match ch {
                                        '{' => depth += 1,
                                        '}' => {
                                            depth -= 1;
                                            if depth == 0 {
                                                end_idx = Some(brace_rel + i);
                                                break;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                if let Some(end_rel) = end_idx {
                                    let extracted = &slice[..=end_rel];
                                    attempt_src = Some(extracted.to_string());
                                }
                            }
                        } else if let Some(no_mangle) = s_raw.find("#[no_mangle]") {
                            if let Some(sig_pos) =
                                s_raw[no_mangle..].find("pub extern \"C\" fn compute")
                            {
                                let abs = no_mangle + sig_pos;
                                let slice = &s_raw[abs..];
                                if let Some(brace_rel) = slice.find('{') {
                                    let mut depth: i32 = 0;
                                    let mut end_idx: Option<usize> = None;
                                    for (i, ch) in slice[brace_rel..].char_indices() {
                                        match ch {
                                            '{' => depth += 1,
                                            '}' => {
                                                depth -= 1;
                                                if depth == 0 {
                                                    end_idx = Some(brace_rel + i);
                                                    break;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    if let Some(end_rel) = end_idx {
                                        attempt_src = Some(slice[..=end_rel].to_string());
                                    }
                                }
                            }
                        }
                    }
                    if let Some(mut clean) = attempt_src {
                        
                        use crate::cleaning::native::{clean as passes_clean, NativeContext, WasmMode};
                        #[allow(unused)]
                        fn detect_mode() -> WasmMode {
                            #[cfg(all(feature = "dynamic-wasi", not(feature = "dynamic-native")))]
                            { WasmMode::Wasi }
                            #[cfg(all(feature = "dynamic-native", not(feature = "dynamic-wasi")))]
                            { WasmMode::Native }
                            #[cfg(all(feature = "dynamic-native", feature = "dynamic-wasi"))]
                            { WasmMode::Native }
                            #[cfg(all(not(feature = "dynamic-native"), not(feature = "dynamic-wasi")))]
                            { compile_error!("At least one of dynamic-native or dynamic-wasi must be enabled"); }
                        }
                        if let Some(pc) = passes_clean(&clean, &NativeContext { directive: directive.to_string(), wasm_mode: detect_mode() }) {
                            clean = pc;
                        }
                        if clean.matches("pub extern \"C\" fn compute").count() > 1
                            || clean.contains("let result: f64 = { {")
                        {
                            last =
                                Some(anyhow::anyhow!("nested compute or embedded JSON artifact"));
                            feedback = Some("nested_or_embedded_artifact".to_string());
                            continue;
                        }
                        #[cfg(all(feature = "dynamic-wasi", not(feature = "dynamic-native")))]
                        {
                            clean = post_sanitize_code(&clean);
                            clean = enforce_exponent_intent(&clean, directive);
                            clean = enforce_reciprocal_intent(&clean, directive);
                        }
                        if !clean.contains("pub extern \"C\" fn compute") {
                            clean = wrap_body_as_compute(&clean);
                        }
                        if clean.contains("copy_nonoverlap(") {
                            clean = clean.replace("copy_nonoverlap(", "copy_nonoverlapping(");
                        }
                        if clean.contains("\n    0\n    return 0;") {
                            clean = clean.replace("\n    0\n    return 0;", "\n    return 0;");
                        }
                        if clean.contains("\n0\nreturn 0;") {
                            clean = clean.replace("\n0\nreturn 0;", "\nreturn 0;");
                        }
                        if let Some(contract_pos) = clean.rfind("// RESULT:") {
                            let contract_line = clean[contract_pos..]
                                .lines()
                                .next()
                                .unwrap_or("")
                                .to_string();
                            let var = contract_line
                                .split(':')
                                .nth(1)
                                .map(|s| s.trim().to_string())
                                .unwrap_or_default();
                            if !var.is_empty() {
                                let needle1 = "let result: f64 = result as f64;";
                                if clean.contains(needle1) {
                                    if var == "result" {
                                        clean = clean.replace(needle1, "");
                                    } else {
                                        clean = clean.replace(
                                            needle1,
                                            &format!("let result: f64 = {var} as f64;"),
                                        );
                                    }
                                }
                                let needle2 = "let result = result as f64;";
                                if clean.contains(needle2) {
                                    if var == "result" {
                                        clean = clean.replace(needle2, "");
                                    } else {
                                        clean = clean.replace(
                                            needle2,
                                            &format!("let result = {var} as f64;"),
                                        );
                                    }
                                }
                                let needle3 = "let result: f64 = result;";
                                if clean.contains(needle3) {
                                    if var == "result" {
                                        clean = clean.replace(needle3, "");
                                    } else {
                                        clean = clean.replace(
                                            needle3,
                                            &format!("let result: f64 = {var} as f64;"),
                                        );
                                    }
                                }
                                for pat in [
                                    "let  result : f64 = result as f64;",
                                    "let result : f64 = result as f64;",
                                    "let result: f64 =  result as f64;",
                                ]
                                .iter()
                                {
                                    if var == "result" && clean.contains(pat) {
                                        clean = clean.replace(pat, "");
                                    }
                                }
                            }
                        }
                        if clean.contains("result.to_string()")
                            && clean.contains("serde_json::to_string(&result)")
                        {
                            if let Some(start_idx) = clean.find("let result_str") {
                                let marker_candidates = [
                                    "let json = match serde_json::to_string(&result)",
                                    "let json = serde_json::to_string(&result)",
                                    "match serde_json::to_string(&result)",
                                ];
                                let mut next_idx: Option<usize> = None;
                                for m in marker_candidates.iter() {
                                    if let Some(pos) = clean[start_idx + 20..].find(m) {
                                        next_idx = Some(start_idx + 20 + pos);
                                        break;
                                    }
                                }
                                if next_idx.is_none() {
                                    if let Some(pos2) = clean[start_idx + 20..]
                                        .find("let result: f64 = result as f64;")
                                    {
                                        next_idx = Some(start_idx + 20 + pos2);
                                    }
                                }
                                if let Some(end_idx) = next_idx {
                                    let to_remove = &clean[start_idx..end_idx];
                                    if to_remove.contains("copy_nonoverlapping") {
                                        clean = clean.replacen(to_remove, "", 1);
                                    }
                                }
                            }
                        }
                        return Ok(RustFnArtifact {
                            code: clean,
                            args: args_vec,
                            magnitude,
                            system_prompt: system.to_string(),
                        });
                    } else {
                        last = Some(anyhow::anyhow!("single-key fallback code cleaning failed"));
                        feedback = Some("single_key_cleaning_failed".to_string());
                        continue;
                    }
                }
            }
        }
        last = Some(anyhow::anyhow!(format!(
            "missing code key attempt {attempt}"
        )));
        feedback = Some("missing_code_key".to_string());
    }
    if let Some(e) = last {
        return Err(e);
    }
    Err(anyhow::anyhow!("unreachable"))
}
