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


use serde_json::Value;


pub struct CleanRust {
    pub source: String,
    pub wrapped: bool,
}



pub fn clean_rust_source(raw: &str) -> Option<CleanRust> {
    fn recurse(input: &str) -> Option<String> {
        let mut s = input.trim().to_string();
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
            if let Ok(val) = serde_json::from_str::<Value>(&s) {
                if let Some(code_str) = val.get("code").and_then(|v| v.as_str()) {
                    return recurse(code_str);
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
        if let Some(idx) = s.find("}\",\"args\":") {
            s = s[..idx + 1].to_string();
        }
        if let Some(idx) = s.find("}\",\"magnitude\":") {
            s = s[..idx + 1].to_string();
        }
        if let Some(idx) = s.find("\n,\"args\":") {
            s = s[..idx].to_string();
        }
        let mut fixed = Vec::new();
        for line in s.lines() {
            let mut l = line.to_string();
            
            if l.contains("sum::<f64>()")
                && (l.contains("..=") || l.contains(".."))
                && !l.contains("map(|x|")
            {
                l = l.replace(".sum::<f64>()", ".map(|x| x as f64).sum::<f64>()");
            }
            
            if l.contains(": f64")
                && l.contains(".sum()")
                && (l.contains("..=") || l.contains(".."))
                && !l.contains("map(|x|")
            {
                l = l.replace(".sum()", ".map(|x| x as f64).sum::<f64>()");
            }
            
            if (l.contains(".sum::<f64>()") || l.contains(".sum::< f64 >()"))
                && l.contains("map(|x| x * x)")
                && !l.contains("(x * x) as f64")
            {
                l = l.replace("map(|x| x * x)", "map(|x| (x * x) as f64)");
            }
            
            if (l.contains(".sum::<f64>()") || l.contains(".sum::< f64 >()"))
                && l.contains("map(|n| n * n)")
                && !l.contains("(n * n) as f64")
            {
                l = l.replace("map(|n| n * n)", "map(|n| (n * n) as f64)");
            }
            
            if l.contains("powi(") && l.contains("-1.0") {
                
                for base in [
                    "(-1.0_f64).powi(",
                    "(-1.0).powi(",
                    "-1.0_f64.powi(",
                    "-1.0.powi(",
                ] {
                    if let Some(start) = l.find(base) {
                        let after = &l[start + base.len()..];
                        if let Some(end) = after.find(')') {
                            let inner = after[..end].trim();
                            
                            
                            let token = if let Some(space) = inner.find(' ') {
                                &inner[..space]
                            } else {
                                inner
                            };
                            
                            if token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                                let replacement =
                                    format!("if (({inner}) % 2)==0 {{1.0_f64}} else {{-1.0_f64}} ");
                                let original_segment = &l[start..start + base.len() + end + 1];
                                l = l.replacen(original_segment, &replacement, 1);
                                break; 
                            }
                        }
                    }
                }
            }
            fixed.push(l);
        }
        Some(fixed.join("\n"))
    }
    let mut cleaned = recurse(raw)?;
    let wrapped = if !cleaned.contains("pub extern \"C\" fn compute") {
        cleaned = wrap_body_as_compute(&cleaned);
        true
    } else {
        false
    };
    Some(CleanRust {
        source: cleaned,
        wrapped,
    })
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
