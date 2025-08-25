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



pub fn try_clean_code(input: &str) -> Option<String> {
    let mut s = input.trim().to_string();
    if s.is_empty() {
        return None;
    }

    if s.starts_with("```") {
        if let Some(pos) = s.find('\n') {
            s = s[pos + 1..].to_string();
        }
        if let Some(end) = s.rfind("```") {
            s = s[..end].to_string();
        }
        s = s.trim().to_string();
    }

    if s.starts_with('{') {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&s) {
            if let Some(inner) = val.get("code").and_then(|v| v.as_str()) {
                return try_clean_code(inner);
            }
        }
    }

    if s.contains("{\"code\":\"") {
        if let Some(idx) = s.find("{\"code\":\"") {
            let after = &s[idx + 9..];
            if let Some(endq) = after.rfind('"') {
                let candidate = &after[..endq];
                let mut cleaned = candidate.replace("\\n", "\n").replace("\\t", "\t");
                cleaned = cleaned.replace("\\\"", "\"");
                s = cleaned;
            }
        }
    }
    if s.contains("\"code\":\"") {
        s = s.replace("\"code\":\"", "");
    }

    if s.contains("let result: f64 = { {") && s.contains("\"code\":") {
        if let Some(outer_start) = s.find("let result: f64 = { {") {
            if let Some(end_block_rel) = s[outer_start..].find("}} };") {
                let end_block = outer_start + end_block_rel + 5;
                let inner = &s[outer_start..end_block];
                if let Some(pos_inner) = inner.rfind("let result =") {
                    let abs_pos = outer_start + pos_inner;
                    if let Some(eq_pos) = s[abs_pos..].find('=') {
                        let rhs_start = abs_pos + eq_pos + 1;
                        if let Some(semi_pos) = s[rhs_start..].find(';') {
                            let rhs = s[rhs_start..rhs_start + semi_pos].trim();
                            let mut rebuilt = String::new();
                            rebuilt.push_str(&s[..outer_start]);
                            rebuilt.push_str("let result: f64 = ");
                            rebuilt.push_str(rhs);
                            rebuilt.push_str(";\n");
                            rebuilt.push_str(&s[end_block..]);
                            s = rebuilt;
                        }
                    }
                }
            }
        }
    }

    if s.matches("pub extern \"C\" fn compute").count() > 1
        || s.matches("pub extern \"C\" fn compute").count() == 0
            && s.matches("pub extern \"C\" fn compute").count() == 0
    {
        if let Some(first) = s.find("pub extern \"C\" fn compute") {
            if let Some(second) = s[first + 1..].find("pub extern \"C\" fn compute") {
                let second_abs = first + 1 + second;

                if let Some(brace_pos) = s[second_abs..].find('{') {
                    let start = second_abs + brace_pos;
                    let mut depth = 0i32;
                    let mut end_idx = None;
                    for (i, ch) in s[start..].char_indices() {
                        if ch == '{' {
                            depth += 1;
                        } else if ch == '}' {
                            depth -= 1;
                            if depth == 0 {
                                end_idx = Some(start + i);
                                break;
                            }
                        }
                    }
                    if let Some(end) = end_idx {
                        let extracted = &s[second_abs..=end];
                        s = extracted.replace("\\\"", "\"");
                    }
                }
            }
        }
    }

    if s.contains("\"code\":") && s.contains("let result: f64 = { {") {
        return None;
    }

    if (s.contains("\\\"pub extern \\\"C\\\" fn compute")
        || s.contains("\\npub extern \\\"C\\\" fn compute"))
        && !s.contains("pub extern \"C\" fn compute")
    {
        let mut unescaped = s.replace("\\r", "");
        unescaped = unescaped
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\\"", "\"");
        if let Some(sig) = unescaped.find("pub extern \"C\" fn compute") {
            if let Some(brace_rel) = unescaped[sig..].find('{') {
                let start = sig + brace_rel;
                let mut depth: i32 = 0;
                let mut end_idx: Option<usize> = None;
                for (i, ch) in unescaped[start..].char_indices() {
                    if ch == '{' {
                        depth += 1;
                    } else if ch == '}' {
                        depth -= 1;
                        if depth == 0 {
                            end_idx = Some(start + i);
                            break;
                        }
                    }
                }
                if let Some(end_abs) = end_idx {
                    let extracted = &unescaped[sig..=end_abs];

                    s = extracted.to_string();
                }
            }
        }
    }

    if s.contains("\"code\": \"\n#[no_mangle]")
        || s.matches("#[no_mangle]").count() > 1
        || s.contains("\"pub extern \"C\" fn compute")
    {
        return None;
    }

    if let Some(func_end) = s.rfind("}\n") {
        if s[func_end + 1..].contains("\"args\":") || s[func_end + 1..].contains("\"magnitude\":") {
            s = s[..func_end + 2].to_string();
        }
    }

    for pat in ["}\",\"args\":", "}\",\"magnitude\":"] {
        if let Some(idx) = s.find(pat) {
            s = s[..idx + 1].to_string();
        }
    }

    let mut fixed = Vec::new();
    for line in s.lines() {
        let mut l = line.to_string();
        if l.contains(": f64")
            && l.contains(".sum()")
            && (l.contains("..=") || l.contains(".."))
            && !l.contains("map(|x| x as f64)")
        {
            l = l.replace(".sum()", ".map(|x| x as f64).sum::<f64>()");
        }

        for intg in ["u32", "i32", "u64", "i64", "usize", "isize"] {
            let pat = format!(".sum::<{intg}>()");
            if l.contains(&pat) {
                l = l.replace(&pat, ".sum::<f64>()");
            }
        }
        fixed.push(l);
    }
    s = fixed.join("\n");

    for (pat, repl) in [
        ("(x - mean)(x - mean)", "(x - mean)*(x - mean)"),
        ("(x-mean)(x-mean)", "(x-mean)*(x-mean)"),
        ("(x - mean)(mean)", "(x - mean)*(mean)"),
        ("(x-mean)(mean)", "(x-mean)*(mean)"),
        ("(x - mean)(mean)*mean", "(x - mean)*(x - mean)"),
        ("(x-mean)(mean)*mean", "(x-mean)*(x-mean)"),
    ] {
        if s.contains(pat) {
            s = s.replace(pat, repl);
        }
    }

    if s.contains(")(") {
        let mut out = String::with_capacity(s.len() + 8);
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if i + 1 < b.len() && b[i] == b')' && b[i + 1] == b'(' {
                let prev = if i > 0 { b[i - 1] as char } else { ' ' };
                let next = if i + 2 < b.len() {
                    b[i + 2] as char
                } else {
                    ' '
                };
                if prev == ' ' || next == ' ' {
                    out.push(')');
                    i += 1;
                    continue;
                }
                out.push_str(")*(");
                i += 2;
                continue;
            }
            out.push(b[i] as char);
            i += 1;
        }
        s = out;
    }
    let forbidden = [
        "std::fs",
        "std::net",
        "std::process",
        "std::env",
        "extern crate",
        "include!",
    ];
    if forbidden.iter().any(|k| s.contains(k)) {
        return None;
    }

    let mut balance: i32 = 0;
    for ch in s.chars() {
        match ch {
            '{' => balance += 1,
            '}' => {
                if balance > 0 {
                    balance -= 1
                }
            }
            _ => {}
        }
    }
    if balance > 0 {
        for _ in 0..balance {
            s.push('}');
        }
        s.push('\n');
    }

    if let Some(sig) = s.find("pub extern \"C\" fn compute") {
        let mut depth = 0i32;
        let mut start_body = None;
        let mut end = None;
        for (i, ch) in s[sig..].char_indices() {
            let gi = sig + i;
            if ch == '{' {
                depth += 1;
                if start_body.is_none() {
                    start_body = Some(gi);
                }
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    end = Some(gi);
                    break;
                }
            }
        }
        if let (Some(body_start), Some(body_end)) = (start_body, end) {
            debug_assert!(
                body_start < body_end,
                "compute function span indices invalid"
            );
            let before = &s[..sig];
            let func_all = &s[sig..=body_end];
            let after = &s[body_end + 1..];
            let lines: Vec<String> = func_all.lines().map(|l| l.to_string()).collect();

            let mut depths: Vec<i32> = Vec::with_capacity(lines.len());
            let mut d = 0;
            for line in &lines {
                depths.push(d);
                for c in line.chars() {
                    if c == '{' {
                        d += 1;
                    } else if c == '}' {
                        d -= 1;
                    }
                }
            }

            let mut filtered: Vec<String> = Vec::with_capacity(lines.len());
            let mut skip_manual = false;
            for (idx, line) in lines.into_iter().enumerate() {
                let lt = line.trim();
                let depth_here = *depths.get(idx).unwrap_or(&0);
                if depth_here == 1
                    && (lt.starts_with("let result_str = format!(\"{}\"")
                        || lt.starts_with("let result = format!(\"{}\"")
                        || lt.starts_with("let output = format!(\"{}\"")
                        || lt.starts_with("let output = result.to_string()")
                        || lt.contains(".to_string()") && lt.starts_with("let output ="))
                {
                    skip_manual = true;
                    continue;
                }
                if skip_manual {
                    if depth_here == 1
                        && (lt.starts_with("copy_nonoverlapping")
                            || lt.contains("copy_from_slice")
                            || lt.contains("to_string()")
                            || lt == "return 0;"
                            || lt == "}")
                    {
                        skip_manual = false;
                        if lt == "}" {
                            filtered.push(line);
                        }
                    }
                    continue;
                }
                if depth_here == 1 && lt == "return 0;" {
                    continue;
                }
                filtered.push(line);
            }

            {
                let mut depth2 = 0i32;
                let mut cleaned: Vec<String> = Vec::with_capacity(filtered.len());
                for i in 0..filtered.len() {
                    let line = &filtered[i];
                    let lt = line.trim();

                    let current_depth = depth2;

                    let mut drop_line = false;
                    if current_depth == 1 {
                        if lt
                            .chars()
                            .all(|c| c.is_ascii_digit() || c == '_' || c == '.')
                            && !lt.ends_with(';')
                        {
                            let mut j = i + 1;
                            while j < filtered.len() {
                                if filtered[j].trim().is_empty() {
                                    j += 1;
                                    continue;
                                }
                                break;
                            }
                            if j < filtered.len() {
                                let nxt = filtered[j].trim_start();
                                if nxt.starts_with("let ") {
                                    drop_line = true;
                                }
                            }
                        }

                        if !drop_line && lt.starts_with("let ") && lt.contains(": f64 =") {
                            if let Some(eq_pos) = lt.find('=') {
                                let (lhs, rhs_with_eq) = lt.split_at(eq_pos);
                                let rhs = rhs_with_eq[1..].trim().trim_end_matches(';').trim();

                                let after_let = lhs[4..].trim();
                                if let Some(colon_pos) = after_let.find(':') {
                                    let var = after_let[..colon_pos].trim();
                                    if rhs == var || rhs == format!("{var} as f64") {
                                        drop_line = true;
                                    }
                                }
                            }
                        }
                    }
                    if !drop_line {
                        cleaned.push(line.clone());
                    }

                    for c in line.chars() {
                        if c == '{' {
                            depth2 += 1;
                        } else if c == '}' {
                            depth2 -= 1;
                        }
                    }
                }
                filtered = cleaned;
            }

            let func_close = filtered
                .iter()
                .rposition(|l| l.trim() == "}")
                .unwrap_or(filtered.len() - 1);

            let has_canonical = filtered.iter().any(|l| l.contains("serde_json::to_string"));
            if !has_canonical {
                let mut contract_var: Option<String> = None;
                for raw in after.lines().take(8) {
                    let t = raw.trim();
                    if let Some(rest) = t.strip_prefix("//") {
                        let rest = rest.trim();
                        if let Some(rest2) = rest.strip_prefix("RESULT:") {
                            let ident = rest2.trim();
                            if !ident.is_empty()
                                && ident
                                    .chars()
                                    .next()
                                    .map(|c| c.is_ascii_alphabetic() || c == '_')
                                    .unwrap_or(false)
                                && ident.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                            {
                                contract_var = Some(ident.to_string());
                                break;
                            }
                        }
                    }
                }

                if let Some(cv) = &contract_var {
                    let pat1 = format!("let {cv} ");
                    let pat2 = format!("let mut {cv} ");
                    let bound = filtered
                        .iter()
                        .any(|l| l.contains(&pat1) || l.contains(&pat2));
                    if !bound {
                        contract_var = None;
                    }
                }

                let mut chosen: Option<String> = contract_var;
                if chosen.is_none() {
                    for l in filtered.iter().rev() {
                        if let Some(pos) = l.find("let ") {
                            if let Some(eq) = l[pos + 4..].find('=') {
                                let name = &l[pos + 4..pos + 4 + eq];
                                if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                                    chosen = Some(name.trim().to_string());
                                    break;
                                }
                            }
                        }
                    }
                    if chosen.is_none() {
                        for l in filtered.iter().rev() {
                            if let Some(plus_pos) = l.find("+=") {
                                let lhs = l[..plus_pos].trim_end();
                                let mut chars = lhs.chars().rev();
                                let mut name_rev = String::new();
                                for ch in chars.by_ref() {
                                    if ch.is_ascii_alphanumeric() || ch == '_' {
                                        name_rev.push(ch);
                                    } else {
                                        break;
                                    }
                                }
                                if !name_rev.is_empty() {
                                    chosen = Some(name_rev.chars().rev().collect());
                                    break;
                                }
                            }
                        }
                    }
                }
                let var = chosen.unwrap_or_else(|| "result".to_string());

                if var == "result" && !filtered.iter().any(|l| l.contains("let result")) {
                    let mut source: Option<String> = None;
                    for l in filtered.iter().rev() {
                        if let Some(pos) = l.find("let ") {
                            if l.contains('=') && l[pos..].chars().any(|c| c.is_ascii_digit()) {
                                if let Some(eq) = l[pos + 4..].find('=') {
                                    let name = &l[pos + 4..pos + 4 + eq];
                                    if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                                        source = Some(name.trim().to_string());
                                        break;
                                    }
                                }
                            }
                        }
                        if source.is_none() && l.contains("+=") {
                            if let Some(plus_pos) = l.find("+=") {
                                let lhs = l[..plus_pos].trim_end();
                                let mut rev = lhs.chars().rev();
                                let mut acc_rev = String::new();
                                for ch in rev.by_ref() {
                                    if ch.is_ascii_alphanumeric() || ch == '_' {
                                        acc_rev.push(ch);
                                    } else {
                                        break;
                                    }
                                }
                                if !acc_rev.is_empty() {
                                    source = Some(acc_rev.chars().rev().collect());
                                    break;
                                }
                            }
                        }
                    }
                    if let Some(src) = source {
                        filtered.insert(func_close, format!("    let result = {src};"));
                    }
                }

                let ser = [
                    format!("    let {var}: f64 = {var} as f64;"),
                    format!("    let json = match serde_json::to_string(&{var}) {{ Ok(v)=>v, Err(_)=> return 4 }};"),
                    "    let out = unsafe { core::slice::from_raw_parts_mut(out_ptr,out_len) };".to_string(),
                    "    if json.as_bytes().len()+1 > out.len() { return 5; }".to_string(),
                    "    let bl = json.as_bytes().len();".to_string(),
                    "    out[..bl].copy_from_slice(json.as_bytes());".to_string(),
                    "    out[bl] = 0;".to_string(),
                    "    return 0;".to_string(),
                ];
                for (offset, line) in ser.into_iter().enumerate() {
                    filtered.insert(func_close + offset, line);
                }

                {
                    let mut depth_tmp = 0i32;
                    let mut cleaned: Vec<String> = Vec::with_capacity(filtered.len());
                    for i in 0..filtered.len() {
                        let line = &filtered[i];
                        let lt = line.trim();
                        let current_depth = depth_tmp;
                        let is_numeric_literal = !lt.is_empty()
                            && lt
                                .chars()
                                .all(|c| c.is_ascii_digit() || c == '_' || c == '.')
                            && !lt.contains("..")
                            && !lt.ends_with(';');
                        let mut drop = false;
                        if current_depth == 1 && is_numeric_literal {
                            let mut j = i + 1;
                            while j < filtered.len() && filtered[j].trim().is_empty() {
                                j += 1;
                            }
                            if j < filtered.len() {
                                let nxt = filtered[j].trim();
                                if nxt.starts_with("let ")
                                    || nxt.starts_with("return ")
                                    || nxt.starts_with("let json")
                                {
                                    drop = true;
                                }
                            } else {
                                drop = true;
                            }
                        }
                        if !drop {
                            cleaned.push(line.clone());
                        }
                        for c in line.chars() {
                            if c == '{' {
                                depth_tmp += 1;
                            } else if c == '}' {
                                depth_tmp -= 1;
                            }
                        }
                    }
                    filtered = cleaned;
                }
            } else if !filtered.iter().any(|l| l.trim() == "return 0;") {
                filtered.insert(func_close, "    return 0;".to_string());
            }
            s = format!("{before}{}{after}", filtered.join("\n"));
        }
    }
    Some(s)
}

pub fn wrap_body_as_compute(body: &str) -> String {
    format!(
        r#"use core::{{slice,str}}; use serde_json::{{self, Value}};
#[no_mangle]
pub extern "C" fn compute(inputs_ptr:*const u8,len:usize,out_ptr:*mut u8,out_len:usize)->i32 {{
    if inputs_ptr.is_null() {{ return 1; }}
    let bytes = unsafe {{ slice::from_raw_parts(inputs_ptr,len) }};
    let s = match str::from_utf8(bytes) {{ Ok(v)=>v, Err(_)=> return 2 }};
    let nums: Vec<f64> = match serde_json::from_str::<Vec<f64>>(s) {{ Ok(v)=>v, Err(_)=> return 3 }};
    let result: f64 = {{ {body} }};
    let json = match serde_json::to_string(&result) {{ Ok(v)=>v, Err(_)=> return 4 }};
    let out = unsafe {{ slice::from_raw_parts_mut(out_ptr,out_len) }};
    if json.as_bytes().len()+1 > out.len() {{ return 5; }}
    let bl = json.as_bytes().len();
    out[..bl].copy_from_slice(json.as_bytes());
    out[bl]=0; 0
}}
"#
    )
}
