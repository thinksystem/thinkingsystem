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



#[cfg(all(feature = "dynamic-wasi", not(feature = "dynamic-native")))]
#[allow(dead_code)]
pub fn enforce_exponent_intent(src: &str, directive: &str) -> String {
    let need_cube = directive.to_ascii_lowercase().contains("cube");
    let d = directive.to_ascii_lowercase();
    let need_fourth = d.contains("fourth") || d.contains("power 4");
    let need_fifth = d.contains("fifth") || d.contains("power 5");
    let mut out = src.to_string();
    if need_cube || need_fourth || need_fifth {
        let mut rebuilt = Vec::new();
        for line in out.lines() {
            if line.contains("map(|x|") && line.contains("sum") {
                let mult_count = line.matches('*').count();
                let target = if need_fifth {
                    5
                } else if need_fourth {
                    4
                } else if need_cube {
                    3
                } else {
                    0
                };
                if target > 0 && mult_count + 1 < target {
                    if let (Some(start_idx), Some(sum_idx)) =
                        (line.find("map(|x|"), line.find(".sum"))
                    {
                        let chain = (0..target).map(|_| "y").collect::<Vec<_>>().join("*");
                        let prefix = &line[..start_idx];
                        let suffix = &line[sum_idx..];
                        let replacement = format!("map(|x| {{ let y = x as f64; {chain} }})");
                        rebuilt.push(format!("{prefix}{replacement}{suffix}"));
                        continue;
                    }
                }
            }
            rebuilt.push(line.to_string());
        }
        out = rebuilt.join("\n");
    }
    out
}
#[cfg(any(not(feature = "dynamic-wasi"), feature = "dynamic-native"))]
#[allow(dead_code)]
pub fn enforce_exponent_intent(src: &str, _: &str) -> String {
    src.to_string()
}

#[cfg(all(feature = "dynamic-wasi", not(feature = "dynamic-native")))]
#[allow(dead_code)]
pub fn post_sanitize_code(input: &str) -> String {
    let mut out_lines = Vec::new();
    for line in input.lines() {
        let mut l = line.to_string();

        if l.contains("map(|x|") && l.contains(".pow(") {
            if let (Some(start_idx), Some(sum_idx)) = (l.find("map(|x|"), l.find(".sum")) {
                let closure = &l[start_idx..sum_idx];
                if let Some(pow_pos) = closure.find(".pow(") {
                    let exp_start = pow_pos + ".pow(".len();
                    let mut exp = String::new();
                    for ch in closure[exp_start..].chars() {
                        if ch.is_ascii_digit() {
                            exp.push(ch);
                        } else {
                            break;
                        }
                    }
                    if let Ok(e) = exp.parse::<usize>() {
                        if (2..=8).contains(&e) {
                            let chain = (0..e).map(|_| "y").collect::<Vec<_>>().join("*");
                            let replacement = format!("map(|x| {{ let y = x as f64; {chain} }})");
                            l = l.replacen(closure, &replacement, 1);
                            if l.contains(".sum::<u64>()") {
                                l = l.replace(".sum::<u64>()", ".sum::<f64>()");
                            }
                            if l.contains(".sum()") && !l.contains(".sum::<f64>()") {
                                l = l.replace(".sum()", ".sum::<f64>()");
                            }
                        }
                    }
                }
            }
        }

        if l.contains(".powi(") {
            for e in [2, 3, 4, 5] {
                let pat = format!(".powi({e})");
                if l.contains(&pat) {
                    if let Some(pos) = l.find(&pat) {
                        let recv = &l[..pos];
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
                            let mult = match e {
                                2 => format!("{base}*{base}"),
                                3 => format!("{base}*{base}*{base}"),
                                4 => format!("({base}*{base})*({base}*{base})"),
                                5 => format!("({base}*{base})*({base}*{base})*{base}"),
                                _ => String::new(),
                            };
                            l = l.replacen(&pat, &format!("({mult})"), 1);
                        }
                    }
                }
            }
        }

        if l.contains("map(|x|") && l.contains("(f64)") {
            let f64_tokens = l.matches("(f64)").count();
            let mut exp = if f64_tokens >= 2 { f64_tokens } else { 2 };
            if exp > 8 {
                exp = 8;
            }
            if let (Some(start_idx), Some(sum_idx)) = (l.find("map(|x|"), l.find(".sum")) {
                let chain = (0..exp).map(|_| "y").collect::<Vec<_>>().join("*");
                let prefix = &l[..start_idx];
                let suffix = &l[sum_idx..];
                l = format!("{prefix}map(|x| {{ let y = x as f64; {chain} }}){suffix}");
            }
        }

        if l.ends_with(");") {
            let mut trimmed = l.clone();
            while trimmed.ends_with("));")
                && trimmed.matches('(').count() + 1 < trimmed.matches(')').count()
            {
                if let Some(pos) = trimmed.rfind("));") {
                    trimmed.replace_range(pos..pos + 3, ");");
                } else {
                    break;
                }
            }
            l = trimmed;
        }

        if l.contains(".sum") {
            if let Some(sum_pos) = l.find(".sum") {
                let (before, after) = l.split_at(sum_pos);
                let mut segment = before.to_string();
                let mut open = segment.matches('(').count();
                let mut close = segment.matches(')').count();
                if close > open {
                    let mut chars: Vec<char> = segment.chars().collect();
                    let mut i = chars.len();
                    while close > open && i > 0 {
                        i -= 1;
                        if chars[i] == ')' {
                            chars.remove(i);
                            close -= 1;
                        }
                    }
                    segment = chars.into_iter().collect();
                }
                l = format!("{}{}", segment, after);
            }
        }

        if l.contains("(f64)") && !l.contains(" as f64") {
            let mut cleaned = String::with_capacity(l.len());
            let mut i = 0;
            while i < l.len() {
                if l[i..].starts_with("(f64)") {
                    i += 6;
                    continue;
                }
                cleaned.push(l.as_bytes()[i] as char);
                i += 1;
            }
            l = cleaned;
        }
        out_lines.push(l);
    }
    out_lines.join("\n")
}
#[cfg(any(not(feature = "dynamic-wasi"), feature = "dynamic-native"))]
#[allow(dead_code)]
pub fn post_sanitize_code(input: &str) -> String {
    input.to_string()
}

#[cfg(all(feature = "dynamic-wasi", not(feature = "dynamic-native")))]
#[allow(dead_code)]
pub fn enforce_reciprocal_intent(src: &str, directive: &str) -> String {
    let d = directive.to_ascii_lowercase();
    if !(d.contains("1 over") || d.contains("reciprocal")) {
        return src.to_string();
    }
    let mut out = String::new();
    let mut changed = false;
    for line in src.lines() {
        if line.contains("map(|x|") && !line.contains("1.0/") && line.contains("x as f64") {
            if line.matches("x as f64").count() == 1
                && (line.contains("y*y") || line.contains("pow"))
            {
                let replaced = line.replace("map(|x|", "map(|x| 1.0/ ");
                out.push_str(&replaced);
                out.push('\n');
                changed = true;
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    if changed {
        out
    } else {
        src.to_string()
    }
}
#[cfg(any(not(feature = "dynamic-wasi"), feature = "dynamic-native"))]
#[allow(dead_code)]
pub fn enforce_reciprocal_intent(src: &str, _: &str) -> String {
    src.to_string()
}
