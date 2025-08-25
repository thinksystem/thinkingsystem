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



use std::fmt;

#[derive(Debug)]
pub struct GuardResult {
    pub source: String,
}

#[derive(Debug)]
pub enum GuardError {
    Rejected(Vec<String>),
}
impl fmt::Display for GuardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GuardError::Rejected(v) => write!(f, "Rejected dynamic Rust: {}", v.join("; ")),
        }
    }
}
impl std::error::Error for GuardError {}

pub fn guard_and_rewrite(raw: &str) -> Result<GuardResult, GuardError> {
    let original = raw.trim();
    let mut reasons = Vec::new();

    // First perform limited, deterministic rewrites (no best-effort loops):
    let mut src = original.to_string();
    // 1. Parity rewrite for (-1.0[_f64]).powi(k)
    if src.contains("powi(") && src.contains("-1.0") {
        let mut out = String::with_capacity(src.len());
        for line in src.lines() {
            let mut l = line.to_string();
            if l.contains("powi(") && (l.contains("-1.0_f64") || l.contains("-1.0")) {
                if let Some(idx) = l.find(".powi(") {
                    if let Some(end) = l[idx + 6..].find(')') {
                        // 6 = len(".powi(")
                        let inner = &l[idx + 6..idx + 6 + end];
                        let repl = format!("if (({inner}) % 2)==0 {{1.0_f64}} else {{-1.0_f64}}");
                        // attempt to include base token `-1.0` or `(-1.0_f64)` preceding
                        let start_search = &l[..idx];
                        let base_start = start_search
                            .rfind("(-1.0_f64)")
                            .or_else(|| start_search.rfind("(-1.0)"))
                            .or_else(|| start_search.rfind("-1.0_f64"))
                            .or_else(|| start_search.rfind("-1.0"));
                        if let Some(bs) = base_start {
                            l = format!("{}{}{}", &l[..bs], repl, &l[idx + 6 + end + 1..]);
                        }
                    }
                }
            }
            out.push_str(&l);
            out.push('\n');
        }
        src = out;
    }
    // 2. Inline small exponent powi(x,2|3|4)
    if src.contains(".powi(") {
        let mut out = String::with_capacity(src.len());
        for line in src.lines() {
            let mut l = line.to_string();
            for exp in [2, 3, 4] {
                let pat = format!(".powi({exp})");
                if l.contains(&pat) {
                    if let Some(pos) = l.find(&pat) {
                        let recv = &l[..pos];
                        // backscan simple identifier
                        let mut start = recv.len();
                        for (i, c) in recv.char_indices().rev() {
                            if !(c.is_alphanumeric() || c == '_' || c == ')') {
                                start = i + 1;
                                break;
                            }
                            if i == 0 {
                                start = 0;
                            }
                        }
                        let base = recv[start..].trim_end_matches('.');
                        if !base.is_empty() {
                            let mult = match exp {
                                2 => format!("{base}*{base}"),
                                3 => format!("{base}*{base}*{base}"),
                                4 => format!("({base}*{base})*({base}*{base})"),
                                _ => unreachable!(),
                            };
                            l = l.replacen(&pat, &format!("({mult})"), 1);
                        }
                    }
                }
            }
            out.push_str(&l);
            out.push('\n');
        }
        src = out;
    }

    const FORBIDDEN: &[&str] = &[
        "std::fs::",
        "std::net::",
        "std::process::",
        "std::thread::",
        "std::env::",
        "env::var",
        "Command::new",
        "TcpListener",
        "TcpStream",
        "UdpSocket",
        "File::open",
        "OpenOptions",
        "rand::",
        "thread_rng()",
        "SystemTime",
        "Instant::now",
        "chrono::Utc::now",
        "std::sync::",
        "std::cell::RefCell",
        "Mutex<",
        "RwLock<",
    ];
    for pat in FORBIDDEN.iter() {
        if src.contains(pat) {
            reasons.push(format!("forbidden pattern: {pat}"));
        }
    }

    const NARROW_TYPES: &[&str] = &[
        ": i32", ": u32", ": i16", ": u16", ": i8", ": u8", "i32::", "u32::", " as i32", " as u32",
    ];
    for pat in NARROW_TYPES.iter() {
        if src.contains(pat) {
            reasons.push(format!("narrow integer disallowed: {pat}"));
        }
    }

    if src.contains(": f32") || src.contains(" f32::") || src.contains(" as f32") {
        reasons.push("f32 not allowed; use f64".into());
    }

    if src.contains(" while ") || src.contains("while(") {
        reasons.push("while loops not allowed".into());
    }
    if src.contains(" loop ") {
        reasons.push("loop construct not allowed".into());
    }

    for line in src.lines() {
        let lt = line.trim();
        if lt.starts_with("for ")
            && !(lt.contains(" in ") && (lt.contains("..=") || lt.contains("..")))
        {
            reasons.push(format!("unbounded or invalid for loop: {lt}"));
        }
    }

    if !src.contains("f64") && !src.contains(" as f64") && !src.contains("0.0") {
        reasons.push("no f64 usage detected".into());
    }

    let line_count = src.lines().count();
    if line_count > 120 {
        reasons.push(format!("too many lines: {line_count}"));
    }

    // Macro restrictions
    for macro_pat in ["println!", "macro_rules!"].iter() {
        if src.contains(macro_pat) {
            reasons.push(format!("macro not allowed: {macro_pat}"));
        }
    }

    // Disallow generic pow( / powf but allow powi for small integer exponents (already expanded earlier) – if any remain that's an error.
    if src.contains(".powf(") || src.contains(".pow(") {
        reasons.push("floating pow disallowed: use explicit multiplication".into());
    }
    if src.contains(".powi(") {
        
        reasons.push("unexpanded powi detected".into());
    }

    
    let externs: Vec<&str> = src.lines().filter(|l| l.contains("extern \"C\"")).collect();
    for l in externs.iter() {
        if !l.contains("pub extern \"C\" fn compute(") {
            reasons.push("extraneous extern signature".into());
        }
    }
    if externs.len() > 1 {
        reasons.push("multiple extern blocks".into());
    }
    
    if src.contains("unsafe ") {
        for line in src.lines() {
            if line.contains("unsafe") {
                
                let trimmed = line.trim();
                if trimmed == "unsafe {" {
                    continue;
                }
                if !(line.contains("from_raw_parts(") || line.contains("from_raw_parts_mut(")) {
                    reasons.push("unsafe usage not allowed".into());
                }
            }
        }
    }

    if reasons.is_empty() {
        Ok(GuardResult { source: src })
    } else {
        Err(GuardError::Rejected(reasons))
    }
}
