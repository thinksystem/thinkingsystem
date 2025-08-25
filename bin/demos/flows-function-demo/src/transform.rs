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



use regex::Regex;

#[derive(Debug, Clone)]
pub struct PassResult {
    pub changed: bool,
}

pub trait WatPass {
    fn run(&self, input: &str) -> (String, PassResult);
}

pub struct RngCanonicalPass;
impl WatPass for RngCanonicalPass {
    fn run(&self, input: &str) -> (String, PassResult) {
        let biased_op = "f64.convert_i32_s";
        let biased_const = "2147483648.0";
        if input.contains(biased_op) && input.contains(biased_const) {
            let out = input
                .replace(biased_op, "f64.convert_i32_u")
                .replace(biased_const, "4294967296.0");
            let changed = out != input;
            return (out, PassResult { changed });
        }
        (input.to_string(), PassResult { changed: false })
    }
}

pub struct IfConditionHoistPass;
impl WatPass for IfConditionHoistPass {
    fn run(&self, input: &str) -> (String, PassResult) {
        let re =
            Regex::new(r#"(?m)^([ \t]*)\(if\s+(\((?:i32\.wrap_i64\s+)?\(call\s+\$[^\)]+\)\)?)\s+"#)
                .unwrap();
        let out = re.replace_all(input, "$1$2\n$1(if ").to_string();
        let changed = out != input;
        (out, PassResult { changed })
    }
}

pub struct Pipeline {
    passes: Vec<Box<dyn WatPass>>,
}
impl Pipeline {
    pub fn new_default() -> Self {
        Self {
            passes: vec![
                Box::new(RngCanonicalPass),
                Box::new(NormalizeModPass),
                Box::new(NormalizeOpcodeNamesPass),
                Box::new(EnsureDefaultMemoryPass),
                Box::new(MemoryAddrWidthPass),
                Box::new(IfConditionHoistPass),
                Box::new(NormalizeIfConditionPass),
                Box::new(ExportStringUnescapePass),
                Box::new(DropBareExprAtBlockEndPass),
                Box::new(EnsureI64TailValuePass),
            ],
        }
    }
    pub fn run(&self, wat: &str) -> (String, Vec<PassResult>) {
        let mut cur = wat.to_string();
        let mut res = Vec::new();
        for p in &self.passes {
            let (next, pr) = p.run(&cur);
            if pr.changed {
                cur = next;
            }
            res.push(pr);
        }
        (cur, res)
    }
}

pub struct NormalizeIfConditionPass;
impl WatPass for NormalizeIfConditionPass {
    fn run(&self, input: &str) -> (String, PassResult) {
        let re_unwrap = Regex::new(r#"\((?:i32\.wrap_i64)\s+(\(i64\.(?:eqz|eq|ne|lt_[us]|gt_[us]|le_[us]|ge_[us])[^)]*\))\)"#).unwrap();
        let mut out = re_unwrap.replace_all(input, "$1").to_string();

        let re_if = Regex::new(r#"(\(if\s+)(\([^\)]+\))"#).unwrap();
        out = re_if
            .replace_all(&out, |caps: &regex::Captures| {
                let prefix = caps.get(1).unwrap().as_str();
                let cond = caps.get(2).unwrap().as_str();
                let t = cond.trim_start();
                let known_i32 = t.starts_with("(i32.")
                    || t.starts_with("(i64.eq")
                    || t.starts_with("(i64.ne")
                    || t.starts_with("(i64.lt")
                    || t.starts_with("(i64.gt")
                    || t.starts_with("(i64.le")
                    || t.starts_with("(i64.ge")
                    || t.starts_with("(i32.wrap_i64 ")
                    || t.starts_with("(i32.eqz ");
                let likely_i64 = t.starts_with("(i64.")
                    || t.starts_with("(local.get")
                    || t.starts_with("(global.get")
                    || t.starts_with("(call");
                if !known_i32 && likely_i64 {
                    format!("{prefix}(i32.wrap_i64 {cond} )")
                } else {
                    format!("{prefix}{cond}")
                }
            })
            .to_string();

        (
            out.clone(),
            PassResult {
                changed: out != input,
            },
        )
    }
}

pub fn run_pass_pipeline(wat: &str) -> (String, Vec<PassResult>) {
    Pipeline::new_default().run(wat)
}

pub struct DropBareExprAtBlockEndPass;
impl WatPass for DropBareExprAtBlockEndPass {
    fn run(&self, input: &str) -> (String, PassResult) {
        let mut lines: Vec<String> = input.lines().map(String::from).collect();
        let mut changed = false;
        for i in 0..lines.len() {
            let line_trim = lines[i].trim();
            let is_candidate = line_trim.starts_with('(')
                && line_trim.ends_with(')')
                && (line_trim.starts_with("(i32.")
                    || line_trim.starts_with("(i64.")
                    || line_trim.starts_with("(f32.")
                    || line_trim.starts_with("(f64."))
                && !line_trim.contains("set")
                && !line_trim.contains("call");
            if !is_candidate {
                continue;
            }
            if let Some(next_line) = lines.iter().skip(i + 1).find(|l| !l.trim().is_empty()) {
                if next_line.trim() == ")" {
                    let indent = lines[i]
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .collect::<String>();
                    let wrapped = format!("{indent}(drop {line_trim})");
                    if lines[i] != wrapped {
                        lines[i] = wrapped;
                        changed = true;
                    }
                }
            }
        }
        (lines.join("\n"), PassResult { changed })
    }
}

pub struct EnsureI64TailValuePass;
impl WatPass for EnsureI64TailValuePass {
    fn run(&self, input: &str) -> (String, PassResult) {
        if !input.contains("(result i64") {
            return (input.to_string(), PassResult { changed: false });
        }
        let mut lines: Vec<String> = input.lines().map(|s| s.to_string()).collect();
        let mut changed = false;

        let mut i = 0usize;
        while i < lines.len() {
            if lines[i].contains("(func ") && lines[i].contains("(result i64") {
                let _func_indent = lines[i]
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .collect::<String>();
                let mut j = i + 1;
                let mut depth: i32 = 1;

                while j < lines.len() {
                    let mut in_str = false;
                    let mut opens = 0i32;
                    let mut closes = 0i32;
                    for ch in lines[j].chars() {
                        if ch == '"' {
                            in_str = !in_str;
                            continue;
                        }
                        if in_str {
                            continue;
                        }
                        if ch == '(' {
                            opens += 1;
                        }
                        if ch == ')' {
                            closes += 1;
                        }
                    }
                    depth += opens - closes;
                    if depth <= 0 {
                        break;
                    }
                    j += 1;
                }
                if j >= lines.len() {
                    break;
                }

                let mut k = if j > 0 { j - 1 } else { j };
                while k > i && lines[k].trim().is_empty() {
                    k -= 1;
                }
                if k > i {
                    let t = lines[k].trim();

                    if t.starts_with("(drop (i64.const ") && t.ends_with(')') {
                        if let Some(pos) = t.find("(i64.const ") {
                            let inner = &t[pos..t.len() - 1];
                            let indent = lines[k]
                                .chars()
                                .take_while(|c| c.is_whitespace())
                                .collect::<String>();
                            let new_line = format!("{indent}{inner})");
                            if new_line != lines[k] {
                                lines[k] = new_line;
                                changed = true;
                            }
                        }
                    } else if !(t.starts_with("(return ")
                        || t.starts_with("(i64.")
                        || t.starts_with("(local.get ")
                        || t.starts_with("(global.get ")
                        || t.starts_with("(call ")
                        || t.starts_with("(i64.const "))
                    {
                        let indent = lines[j]
                            .chars()
                            .take_while(|c| c.is_whitespace())
                            .collect::<String>();
                        lines.insert(j, format!("{indent}(i64.const 0)"));
                        changed = true;

                        i = j + 1;
                        continue;
                    }
                }
                i = j + 1;
                continue;
            }
            i += 1;
        }

        (lines.join("\n"), PassResult { changed })
    }
}

pub struct EnsureDefaultMemoryPass;
impl WatPass for EnsureDefaultMemoryPass {
    fn run(&self, input: &str) -> (String, PassResult) {
        let uses_mem = input.contains(".load") || input.contains(".store");
        if !uses_mem || input.contains("(memory") {
            return (input.to_string(), PassResult { changed: false });
        }

        let mut out = String::with_capacity(input.len() + 32);
        let mut inserted = false;
        for (idx, line) in input.lines().enumerate() {
            if idx == 0 && line.trim_start().starts_with("(module") {
                out.push_str(line);
                out.push('\n');
                out.push_str("  (memory 1)\n");
                inserted = true;
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        if out.ends_with('\n') {
            out.pop();
        }
        (out, PassResult { changed: inserted })
    }
}

pub struct MemoryAddrWidthPass;
impl WatPass for MemoryAddrWidthPass {
    fn run(&self, input: &str) -> (String, PassResult) {
        if !(input.contains(".load (") || input.contains(".store (")) {
            return (input.to_string(), PassResult { changed: false });
        }
        let ops = ["(i64.load ", "(i32.load ", "(i64.store ", "(i32.store "];
        let mut changed = false;
        let mut out_lines = Vec::with_capacity(input.lines().count());
        for line in input.lines() {
            let mut li = line.to_string();
            let mut progress = true;
            while progress {
                progress = false;
                let mut best_pos: Option<(usize, &str)> = None;
                for op in &ops {
                    if let Some(p) = li.find(op) {
                        best_pos = match best_pos {
                            Some((bp, _)) if bp < p => best_pos,
                            _ => Some((p, *op)),
                        };
                    }
                }
                if let Some((pos, op)) = best_pos {
                    let expr_start = pos + op.len();

                    let after = &li[expr_start..];
                    let offset_ws = after.chars().take_while(|c| c.is_whitespace()).count();
                    let i0 = expr_start + offset_ws;
                    if i0 >= li.len() || li.as_bytes()[i0] as char != '(' {
                        let _advance = pos + op.len();
                        break;
                    }

                    let snapshot = li.clone();
                    let bytes = snapshot.as_bytes();
                    let mut k = i0;
                    let mut depth: i32 = 0;
                    while k < bytes.len() {
                        let ch = bytes[k] as char;
                        if ch == '(' {
                            depth += 1;
                        }
                        if ch == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        k += 1;
                    }
                    if k >= bytes.len() {
                        break;
                    }
                    let inner = &snapshot[i0..=k];
                    let inner_trim = inner.trim_start();
                    if (inner_trim.starts_with("(i64.")
                        || inner_trim.starts_with("(local.get $")
                        || inner_trim.starts_with("(global.get $"))
                        && !inner_trim.starts_with("(i32.")
                        && !inner_trim.starts_with("(i32.wrap_i64 ")
                    {
                        let wrapped = format!("(i32.wrap_i64 {inner} )");
                        li.replace_range(i0..=k, &wrapped);
                        changed = true;
                        progress = true;
                        continue;
                    }

                    
                }
            }
            out_lines.push(li);
        }
        (out_lines.join("\n"), PassResult { changed })
    }
}

pub fn pre_sanitize_structural_fix(input: &str) -> Option<String> {
    let mut changed = false;
    let mut out = input.trim_start_matches('\u{feff}').to_string();
    let trimmed = out.trim_start();
    if !trimmed.starts_with("(module") {
        out = format!("(module\n{out}\n)");
        changed = true;
    }

    let mut opens: i64 = 0;
    for ch in out.chars() {
        if ch == '(' {
            opens += 1;
        } else if ch == ')' {
            opens -= 1;
        }
    }
    if opens > 0 {
        out.push_str(&")".repeat(opens as usize));
        changed = true;
    }
    if changed {
        Some(out)
    } else {
        None
    }
}

pub struct ExportStringUnescapePass;
impl WatPass for ExportStringUnescapePass {
    fn run(&self, input: &str) -> (String, PassResult) {
        if !input.contains("(export ") {
            return (input.to_string(), PassResult { changed: false });
        }
        let mut changed = false;
        let mut out_lines = Vec::with_capacity(input.lines().count());
        for line in input.lines() {
            if line.contains("(export ") {
                let mut new_line = String::with_capacity(line.len());
                let mut in_str = false;
                let mut i = 0usize;
                let bytes = line.as_bytes();
                while i < bytes.len() {
                    let c = bytes[i] as char;
                    if c == '"' {
                        in_str = !in_str;
                        new_line.push(c);
                        i += 1;
                        continue;
                    }
                    if in_str && c == '\\' && i + 1 < bytes.len() {
                        let n = bytes[i + 1] as char;
                        match n {
                                '"' => {
                                    new_line.push('"');
                                    i += 2;
                                    changed = true;
                                    continue;
                                }
                                'n' => {
                                    i += 2;
                                    changed = true;
                                    continue;
                                }
                                'r' => {
                                    i += 2;
                                    changed = true;
                                    continue;
                                }
                                't' => {
                                    new_line.push(' ');
                                    i += 2;
                                    changed = true;
                                    continue;
                                }
                                _ => {
                                    new_line.push(n);
                                    i += 2;
                                    changed = true;
                                    continue;
                                }
                        }
                    }
                    new_line.push(c);
                    i += 1;
                }
                if new_line != line {
                    changed = true;
                }
                out_lines.push(new_line);
            } else {
                out_lines.push(line.to_string());
            }
        }
        (out_lines.join("\n"), PassResult { changed })
    }
}

pub struct NormalizeModPass;
impl WatPass for NormalizeModPass {
    fn run(&self, input: &str) -> (String, PassResult) {
        if !(input.contains("i64.mod") || input.contains("i32.mod")) {
            return (input.to_string(), PassResult { changed: false });
        }
        let out = input
            .replace("i64.mod", "i64.rem_u")
            .replace("i32.mod", "i32.rem_u");
        let changed = out != input;
        (out, PassResult { changed })
    }
}

pub struct NormalizeOpcodeNamesPass;
impl WatPass for NormalizeOpcodeNamesPass {
    fn run(&self, input: &str) -> (String, PassResult) {
        let mut out = input.to_string();

        out = out
            .replace("f64.convert_u_i64", "f64.convert_i64_u")
            .replace("f64.convert_s_i64", "f64.convert_i64_s")
            .replace("f32.convert_u_i64", "f32.convert_i64_u")
            .replace("f32.convert_s_i64", "f32.convert_i64_s")
            .replace("f64.convert_u_i32", "f64.convert_i32_u")
            .replace("f64.convert_s_i32", "f64.convert_i32_s")
            .replace("f32.convert_u_i32", "f32.convert_i32_u")
            .replace("f32.convert_s_i32", "f32.convert_i32_s");

        out = out
            .replace("f64.convert_u/i64", "f64.convert_i64_u")
            .replace("f64.convert_s/i64", "f64.convert_i64_s")
            .replace("f32.convert_u/i64", "f32.convert_i64_u")
            .replace("f32.convert_s/i64", "f32.convert_i64_s")
            .replace("f64.convert_u/i32", "f64.convert_i32_u")
            .replace("f64.convert_s/i32", "f64.convert_i32_s")
            .replace("f32.convert_u/i32", "f32.convert_i32_u")
            .replace("f32.convert_s/i32", "f32.convert_i32_s")
            .replace("f64.convert_u-i64", "f64.convert_i64_u")
            .replace("f64.convert_s-i64", "f64.convert_i64_s")
            .replace("f32.convert_u-i64", "f32.convert_i64_u")
            .replace("f32.convert_s-i64", "f32.convert_i64_s")
            .replace("f64.convert_u-i32", "f64.convert_i32_u")
            .replace("f64.convert_s-i32", "f64.convert_i32_s")
            .replace("f32.convert_u-i32", "f32.convert_i32_u")
            .replace("f32.convert_s-i32", "f32.convert_i32_s");

        out = out
            .replace("i64.trunc_u_f64", "i64.trunc_f64_u")
            .replace("i64.trunc_s_f64", "i64.trunc_f64_s")
            .replace("i64.trunc_u_f32", "i64.trunc_f32_u")
            .replace("i64.trunc_s_f32", "i64.trunc_f32_s")
            .replace("i32.trunc_u_f64", "i32.trunc_f64_u")
            .replace("i32.trunc_s_f64", "i32.trunc_f64_s")
            .replace("i32.trunc_u_f32", "i32.trunc_f32_u")
            .replace("i32.trunc_s_f32", "i32.trunc_f32_s");

        out = out
            .replace("i64.trunc_u/f64", "i64.trunc_f64_u")
            .replace("i64.trunc_s/f64", "i64.trunc_f64_s")
            .replace("i64.trunc_u/f32", "i64.trunc_f32_u")
            .replace("i64.trunc_s/f32", "i64.trunc_f32_s")
            .replace("i32.trunc_u/f64", "i32.trunc_f64_u")
            .replace("i32.trunc_s/f64", "i32.trunc_f64_s")
            .replace("i32.trunc_u/f32", "i32.trunc_f32_u")
            .replace("i32.trunc_s/f32", "i32.trunc_f32_s")
            .replace("i64.trunc_u-f64", "i64.trunc_f64_u")
            .replace("i64.trunc_s-f64", "i64.trunc_f64_s")
            .replace("i64.trunc_u-f32", "i64.trunc_f32_u")
            .replace("i64.trunc_s-f32", "i64.trunc_f32_s")
            .replace("i32.trunc_u-f64", "i32.trunc_f64_u")
            .replace("i32.trunc_s-f64", "i32.trunc_f64_s")
            .replace("i32.trunc_u-f32", "i32.trunc_f32_u")
            .replace("i32.trunc_s-f32", "i32.trunc_f32_s");

        out = out
            .replace("i64.extend_u_i32", "i64.extend_i32_u")
            .replace("i64.extend_s_i32", "i64.extend_i32_s");

        out = out
            .replace("i64.extend_u/i32", "i64.extend_i32_u")
            .replace("i64.extend_s/i32", "i64.extend_i32_s")
            .replace("i64.extend_u-i32", "i64.extend_i32_u")
            .replace("i64.extend_s-i32", "i64.extend_i32_s");
        let changed = out != input;
        (out, PassResult { changed })
    }
}
