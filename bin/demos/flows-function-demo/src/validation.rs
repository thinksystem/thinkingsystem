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


use stele::nlu::llm_processor::LLMAdapter;
use wat::parse_str as wat_parse;

#[derive(Debug, serde::Deserialize)]
pub struct ValidationVerdict {
    pub accept: bool,
    #[serde(default)]
    pub trivial: bool,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub loops: Option<u32>,
    #[serde(default)]
    pub arithmetic_ops: Option<u32>,
    #[serde(default)]
    pub param_reads: Option<u32>,
}


pub fn is_explicit_constant_directive(directive: &str) -> bool {
    let d = directive.to_lowercase();
    if d.contains("return constant") {
        return true;
    }
    if d.starts_with("return ") {
        let tokens: Vec<&str> = d.split_whitespace().collect();
        if tokens.len() == 2 && tokens[1].chars().any(|c| c.is_ascii_digit()) {
            return true;
        }
    }
    ["output exactly", "just return", "always return"]
        .iter()
        .any(|p| d.contains(p))
}

pub async fn llm_validate_wat(
    adapter: &dyn LLMAdapter,
    directive: &str,
    name: &str,
    wat: &str,
) -> anyhow::Result<ValidationVerdict> {
    let system = r#"You are a STRICT validator of WebAssembly function implementations.
Return ONLY minified JSON object with keys:
    accept: bool
    trivial: bool
    reasons: string[]  (UPPER_SNAKE machine tokens)
    loops: number (loop constructs count)
    arithmetic_ops: number (add/sub/mul/rem/div ops count)
    param_reads: number (local.get of parameters)
Definitions:
    trivial: result independent of inputs OR constant-only body OR lacks any control/iteration while directive semantically implies multiple evaluation steps / sampling / search / accumulation. Infer this from directive semantics (do not rely on fixed keyword lists).
Reason codes (subset as needed): TRIVIAL_CONSTANT, MISSING_LOOP, NO_DATAFLOW, INSUFFICIENT_WORK, NO_PARAM, OTHER
Evaluation outline:
    1. Count loops ( (loop ...) occurrences ). If directive describes aggregation, approximation, search, generation or sampling and loops==0 => MISSING_LOOP.
    2. Count arithmetic ops (add/sub/mul/div/rem). If directive implies numeric computation and count <2 => INSUFFICIENT_WORK.
    3. If function has parameters but none accessed via local.get => NO_PARAM or NO_DATAFLOW.
    4. If body returns a literal constant immediately => TRIVIAL_CONSTANT.
    5. trivial=true if any of TRIVIAL_CONSTANT, NO_DATAFLOW, MISSING_LOOP, INSUFFICIENT_WORK.
Exceptions / nuance:
    - If directive explicitly requests returning a fixed constant (phrases like 'return constant', 'return the number 42', 'output exactly 7', 'just return 0.5', 'always return 1'), then a constant body is acceptable and MUST NOT be marked trivial; in that case: trivial=false, accept=true (unless other structural issues exist).
Acceptance: accept = !trivial (after applying the exception rule above).
Output policy: single-line JSON only.
"#;
    let user =
        format!("Directive: {directive}\nFunction: {name}\nWAT:\n{wat}\nRespond now with JSON.");
    let resp = adapter
        .generate_structured_response(system, &user)
        .await
        .map_err(|e| anyhow::anyhow!("validator call failed: {e}"))?;
    if resp.get("accept").is_some() {
        if let Ok(v) = serde_json::from_value::<ValidationVerdict>(resp.clone()) {
            
            tracing::debug!(function=%name, accept=%v.accept, trivial=%v.trivial, loops=?v.loops, arithmetic_ops=?v.arithmetic_ops, param_reads=?v.param_reads, "validator_raw_verdict");
            return Ok(v);
        }
    }
    for (_k, v) in resp.as_object().into_iter().flat_map(|m| m.iter()) {
        if let Some(s) = v.as_str() {
            if s.contains("accept") && s.contains("trivial") {
                if let Ok(val) = serde_json::from_str::<ValidationVerdict>(s) {
                    return Ok(val);
                }
            }
        }
    }
    Err(anyhow::anyhow!(
        "Validator did not return expected JSON verdict"
    ))
}

pub fn sanitize_and_validate_wat(src: &str, name: &str, export: &str) -> anyhow::Result<String> {
    let mut s = src
        .replace("set_local", "local.set")
        .replace("get_local", "local.get");
    
    
    
    
    const ALLOWED_NUMERIC_PREFIXES: &[&str] = &[
        
        "f64.const",
        "f64.add",
        "f64.sub",
        "f64.mul",
        "f64.div",
        "f64.sqrt",
        
        "f64.lt",
        "f64.gt",
        "f64.le",
        "f64.ge",
        "f64.eq",
        "f64.ne",
        
        "i64.const",
        "i64.add",
        "i64.sub",
        "i64.mul",
        "i64.div_s",
        "i64.div_u",
        "i64.rem_s",
        "i64.rem_u",
        "i64.gt_s",
        "i64.lt_s",
        "i64.eqz",
        "i64.trunc_f64_s",
        "f64.convert_i64_s",
    ];
    
    const FORBIDDEN_OPS: &[&str] = &[
        "f64.root", "f64.pow", "f64.powf", "f64.cbrt", "f64.exp", "f64.ln", "f64.log", "f64.sin",
        "f64.cos", "f64.tan", "f32.root", "f32.pow", "f32.cbrt",
    ];
    let mut forbidden_found: Vec<&'static str> = Vec::new();
    for fb in FORBIDDEN_OPS {
        if s.contains(fb) {
            forbidden_found.push(*fb);
        }
    }
    if !forbidden_found.is_empty() {
        return Err(anyhow::anyhow!(
            "Unsupported opcode(s) {:?} detected – allowed numeric ops: {:?}. Use iterative approximation (e.g. Newton) via add/sub/mul/div/loop instead.",
            forbidden_found, ALLOWED_NUMERIC_PREFIXES
        ));
    }
    // ---- Automatic widening pass (requested): transform any 32-bit surface (types/opcodes/results) to 64-bit BEFORE opcode vetting ----
    if s.contains("f32") || s.contains("i32") {
        // f32 -> f64 straightforward textual substitution
        if s.contains("f32.") || s.contains(" f32") {
            s = s
                .replace("f32.", "f64.")
                .replace(" f32)", " f64)")
                .replace(" f32 ", " f64 ")
                // (param $...) pattern unaffected; no replace needed
                .replace("(result f32)", "(result f64)")
                .replace("f32.const", "f64.const");
        }
        // i32 arithmetic/comparison ops -> i64 equivalents. Leave i32.wrap_i64 intact (semantic narrowing intentionally retained for RNG seeding) but ensure later passes allow it by not scanning for i32 tokens.
        if s.contains("i32.") || s.contains(" i32") {
            for op in [
                "add", "sub", "mul", "div_s", "div_u", "rem_s", "rem_u", "lt_s", "lt_u", "gt_s",
                "gt_u", "le_s", "le_u", "ge_s", "ge_u", "eq", "ne",
            ] {
                let from = format!("i32.{op}");
                let to = format!("i64.{op}");
                s = s.replace(&from, &to);
            }
            s = s
                .replace(" i32)", " i64)")
                .replace(" i32 ", " i64 ")
                .replace("(result i32)", "(result i64)")
                .replace("i32.const", "i64.const");
        }
    }
    // After widening, perform unknown opcode scan (now only f64./i64. expected)
    let mut unknown: Vec<String> = Vec::new();
    for raw in s.split_whitespace() {
        let token = raw.trim_matches(|c| c == '(' || c == ')');
        if token.starts_with("f64.") || token.starts_with("i64.") {
            if ALLOWED_NUMERIC_PREFIXES
                .iter()
                .any(|p| token.starts_with(p))
            {
                continue;
            }
            let i64_extra_cmp = ["gt_u", "lt_u", "ge_s", "ge_u", "le_s", "le_u", "eq", "ne"];
            if token.starts_with("i64.") && i64_extra_cmp.iter().any(|suf| token.ends_with(suf)) {
                continue;
            }
            unknown.push(token.to_string());
        }
    }
    if !unknown.is_empty() {
        unknown.sort();
        unknown.dedup();
        return Err(anyhow::anyhow!(
            "Unknown / disallowed numeric opcode tokens {:?} – allowed base set: {:?}",
            unknown,
            ALLOWED_NUMERIC_PREFIXES
        ));
    }
    // Ensure no residual 32-bit result declarations remain (should have been widened)
    if s.contains("(result f32)") || s.contains("(result i32)") {
        return Err(anyhow::anyhow!(
            "Residual 32-bit result after widening pass"
        ));
    }
    if s.contains("\\\"") {
        s = s.replace("\\\"", "\"");
    }
    let mut cleaned_lines = Vec::new();
    for line in s.lines() {
        if !line.trim_start().starts_with("(import ") {
            cleaned_lines.push(line);
        }
    }
    s = cleaned_lines.join("\n");
    if s.contains("\"wat\"") && s.contains("(module") && s.trim_start().starts_with('{') {
        if let Some(mod_start) = s.find("(module") {
            if let Some(last_paren) = s.rfind(')') {
                let candidate = &s[mod_start..=last_paren];
                if candidate.starts_with("(module") {
                    s = candidate.to_string();
                }
            }
        }
    }
    if !s.contains("(module") {
        s = format!("(module\n{s}\n)");
    }
    if s.matches("(module").count() > 1 {
        let first = s.find("(module").unwrap();
        let mut out = String::new();
        let mut i = 0usize;
        while i < s.len() {
            if i + 7 <= s.len() && &s[i..i + 7] == "(module" {
                if i == first {
                    out.push_str("(module");
                } else {
                    out.push_str(" ;; removed-nested-module ");
                }
                i += 7;
            } else {
                out.push(s.as_bytes()[i] as char);
                i += 1;
            }
        }
        s = out;
    }
    if !s.contains(&format!("(func ${name}")) {
        if let Some(_idx) = s.find("(func ") {
            s = s.replacen("(func ", &format!("(func ${name} "), 1);
        }
        if !s.contains(&format!("(func ${name}")) {
            s.push_str(&format!(
                "\n  (func ${name} (param $x f64) (result f64) (f64.const 0))"
            ));
        }
    }
    let mut needed_exports = Vec::new();
    if !s.contains(&format!("(export \"{export}\"")) {
        needed_exports.push(format!("  (export \"{export}\" (func ${name}))"));
    }
    if export != name && !s.contains(&format!("(export \"{name}\"")) {
        needed_exports.push(format!("  (export \"{name}\" (func ${name}))"));
    }
    if !needed_exports.is_empty() {
        if let Some(pos) = s.rfind(')') {
            s.insert_str(pos, &format!("\n{}\n", needed_exports.join("\n")));
        }
    }
    if let Some(last) = s.rfind(')') {
        let (left, right) = s.split_at(last + 1);
        if right.trim().starts_with("(export") {
            s = left.to_string();
        }
    }
    if s.contains("(local") {
        let mut fixed = String::new();
        for line in s.lines() {
            let mut l = line.to_string();
            if l.contains("(local") {
                l = l
                    .replace("(i64))", "i64)")
                    .replace("(f64))", "f64)")
                    .replace("(i32))", "i32)");
            }
            fixed.push_str(&l);
            fixed.push('\n');
        }
        s = fixed;
    }
    // ---- Operand shape repair: wrap bare symbol operands after arithmetic opcodes ----
    // LLM sometimes emits forms like: (f64.div $x (f64.mul (local.get $guess) ...)) which are invalid – operands must be expressions.
    // We rewrite (OP $sym ...) => (OP (local.get $sym) ...). Applies to first operand only; subsequent bare symbols typically already appear as local.get.
    fn wrap_bare_operands(module: &str) -> String {
        let ops = [
            "f64.add",
            "f64.sub",
            "f64.mul",
            "f64.div",
            "f64.rem",
            "i64.add",
            "i64.sub",
            "i64.mul",
            "i64.div_s",
            "i64.div_u",
            "i64.rem_s",
            "i64.rem_u",
        ];
        let mut out = String::with_capacity(module.len());
        for line in module.lines() {
            let mut modified = line.to_string();
            for op in &ops {
                // pattern: (op $
                let pattern = format!("({op} $");
                loop {
                    if let Some(idx) = modified.find(&pattern) {
                        // idx points to '(' of pattern
                        let start_sym = idx + pattern.len() - 1; // position of '$'
                                                                 // extract symbol name following '$'
                        let rest = &modified[start_sym + 1..];
                        let sym: String = rest
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .collect();
                        if sym.is_empty() {
                            break;
                        }
                        // Avoid double wrapping: if already '(local.get $sym' immediately after op
                        if rest.starts_with(&format!("{sym} "))
                            || rest.starts_with(&format!("{sym})"))
                        {
                            // Replace "$sym" with "(local.get $sym)"
                            let before = &modified[..start_sym - 1]; // up to space before '$'
                            let after = &modified[start_sym + 1 + sym.len()..];
                            modified = format!("{before}(local.get ${sym}){after}");
                            continue; // search for next occurrence
                        }
                    }
                    break;
                }
            }
            out.push_str(&modified);
            out.push('\n');
        }
        out
    }
    s = wrap_bare_operands(&s);
    // ---- Conditional condition normalization: WebAssembly 'if' expects i32 condition ----
    // LLM frequently emits (if (local.get $flag) (then ...)) where $flag is an i64 used as boolean.
    // This is invalid because the condition must yield an i32. We rewrite such patterns to
    // (if (i64.ne (local.get $flag) (i64.const 0)) ...) which produces an i32 result.
    // We avoid touching already well-formed conditions (those starting with '(' after (if ).
    // Simple line-based heuristic: replace exact substring '(if (local.get $X)' when the following
    // token is a closing paren or whitespace then '(then' later on the line/block.
    if s.contains("(if (local.get $") {
        let mut fixed = String::with_capacity(s.len());
        for line in s.lines() {
            let mut out_line = line.to_string();
            // Scan multiple occurrences per line if any.
            loop {
                if let Some(start) = out_line.find("(if (local.get $") {
                    // Extract symbol name
                    let after = &out_line[start + "(if (local.get $".len()..];
                    let sym: String = after
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    if sym.is_empty() {
                        break;
                    }
                    // Build replacement condition
                    let pattern = format!("(if (local.get ${sym})");
                    if out_line.contains(&pattern) {
                        let replacement = format!("(if (i64.ne (local.get ${sym}) (i64.const 0))");
                        out_line = out_line.replacen(&pattern, &replacement, 1);
                        continue; // look for more occurrences
                    }
                }
                break;
            }
            fixed.push_str(&out_line);
            fixed.push('\n');
        }
        s = fixed;
    }
    wat_parse(&s).map_err(|e| anyhow::anyhow!("WAT parse error: {e}"))?;
    // Basic semantic sanity: disallow undefined helper calls like (call $random) which frequently appear without definition
    if s.contains("(call $random") && !s.contains("(func $random") {
        return Err(anyhow::anyhow!(
            "Undefined helper function $random; implement inline LCG logic instead"
        ));
    }
    // Heuristic: reject unbounded LCG normalization that converts a 64-bit seed directly to f64 without wrapping to 32 bits.
    if s.contains("f64.convert_i64_u") && !s.contains("i32.wrap_i64") {
        return Err(anyhow::anyhow!("Unbounded LCG normalization: wrap seed with i32.wrap_i64 before converting to f64 and scale using 2^31 or 2^32 divisor"));
    }
    Ok(s)
}

// Structural metrics (Item 14) independent of LLM.
#[derive(Debug, Clone)]
pub struct StructuralMetrics {
    pub loops: u32,
    pub arithmetic_ops: u32,
    pub param_reads: u32,
    pub length_bytes: usize,
}

pub fn analyze_structural(wat: &str) -> StructuralMetrics {
    let loops = wat.matches("(loop").count() as u32;
    let arithmetic_ops = [
        // floating
        "f64.add",
        "f64.sub",
        "f64.mul",
        "f64.div",
        "f64.rem",
        // 64-bit integer
        "i64.add",
        "i64.sub",
        "i64.mul",
        "i64.div_s",
        "i64.div_u",
        "i64.rem_s",
        "i64.rem_u",
        // legacy 32-bit integer (still count if present pre-widen)
        "i32.add",
        "i32.sub",
        "i32.mul",
        "i32.div_s",
        "i32.div_u",
        "i32.rem_s",
        "i32.rem_u",
    ]
    .iter()
    .map(|op| wat.matches(op).count() as u32)
    .sum();
    // Generic param extraction: find all (param $name) tokens
    let mut param_symbols = Vec::new();
    for line in wat.lines() {
        if let Some(idx) = line.find("(param $") {
            // may be multiple per line
            let mut rest = &line[idx..];
            while let Some(pos) = rest.find("(param $") {
                let after = &rest[pos + 7..];
                if let Some(name_part) = after.split_whitespace().next() {
                    let clean = name_part.trim_matches(|c: char| c == ')' || c == '(' || c == ',');
                    if !clean.is_empty() {
                        param_symbols.push(clean.to_string());
                    }
                }
                rest = &after[1..];
            }
        }
    }
    let mut param_reads = 0u32;
    for sym in &param_symbols {
        let needle = format!("local.get ${sym}");
        param_reads += wat.matches(&needle).count() as u32;
    }
    StructuralMetrics {
        loops,
        arithmetic_ops,
        param_reads,
        length_bytes: wat.len(),
    }
}

pub fn structurally_confident(metrics: &StructuralMetrics) -> bool {
    metrics.loops > 0
        && metrics.arithmetic_ops >= 2
        && metrics.param_reads >= 1
        && metrics.length_bytes > 120
}

pub async fn attempt_wat_repair(
    adapter: &dyn LLMAdapter,
    directive: &str,
    name: &str,
    export: &str,
    previous_wat: &str,
    error: &str,
) -> anyhow::Result<String> {
    let system = r#"You repair WebAssembly Text (WAT) modules. Output ONLY JSON {"wat":"<module>"}.
Generic requirements:
 - Single (module ...)
 - Provide exactly one (func $NAME ...) where NAME provided; result may be f64 or i64 depending on directive need.
 - Export that function with the given export label and (optionally) also its own name if different.
 - No imports and no calls to undefined helpers (e.g. (call $random) is forbidden). Implement pseudo-random needs inline via a simple Linear Congruential Generator over an i64 state if randomness implied.
 - Use only deterministic instructions (const, local.get/set, add/sub/mul/div/rem, comparison, loop/block/br_if, minimal conversions) necessary for logic.
 - ABSOLUTE BAN: do NOT use any 32-bit numeric types or ops (i32.*, f32.*) anywhere. Only i64 and f64 surfaces are allowed.
 - Function should not just return a constant unless directive explicitly asks for a constant value.
    - If generating uniform random doubles in [-1,1] from a 64-bit seed, FIRST apply (i32.wrap_i64 (local.get $seed)), convert that i32 to f64, then divide by an appropriate power-of-two constant (e.g. 2147483648.0) and scale/shift; NEVER use f64.convert_i64_u directly without wrapping.
 - Return syntactically valid WAT only inside the JSON wrapper; no markdown, no commentary."#;
    let user = format!("Directive: {directive}\nFunction name: {name}\nExport label: {export}\nPrevious invalid WAT:\n{previous_wat}\nError: {error}\nReturn corrected JSON now.");
    let resp = adapter
        .generate_structured_response(system, &user)
        .await
        .map_err(|e| anyhow::anyhow!("LLM WAT repair failure: {e}"))?;
    if let Some(w) = resp.get("wat").and_then(|v| v.as_str()) {
        return Ok(w.to_string());
    }
    for (_k, v) in resp.as_object().into_iter().flat_map(|m| m.iter()) {
        if let Some(s) = v.as_str() {
            if s.contains("(module") {
                return Ok(s.to_string());
            }
        }
    }
    Err(anyhow::anyhow!("LLM did not return repaired wat JSON"))
}
