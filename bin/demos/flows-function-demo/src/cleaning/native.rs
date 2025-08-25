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



pub struct NativeContext {
    pub directive: String,
    pub wasm_mode: WasmMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WasmMode {
    Native,
    Wasi,
}

pub trait CodePass {
    fn run(&self, code: &str, ctx: &NativeContext) -> (String, bool);
}

pub fn clean(code: &str, ctx: &NativeContext) -> Option<String> {
    let passes: Vec<Box<dyn CodePass>> = vec![
        Box::new(StripMarkdownJson),
        Box::new(FixCopyNonoverlapping),
        Box::new(RemoveStrayZeroBeforeReturn),
    ];
    let mut cur = code.to_string();
    let mut changed = false;
    for p in passes {
        let (next, ch) = p.run(&cur, ctx);
        if ch {
            cur = next;
            changed = true;
        }
    }
    if changed { Some(cur) } else { None }
}

struct StripMarkdownJson;
impl CodePass for StripMarkdownJson {
    fn run(&self, code: &str, _ctx: &NativeContext) -> (String, bool) {
        let trimmed = code.trim();
        if trimmed.starts_with("```") && trimmed.ends_with("```") {
            
            let mut lines: Vec<&str> = code.lines().collect();
            if !lines.is_empty() && lines[0].starts_with("```") {
                lines.remove(0);
            }
            if !lines.is_empty() && lines.last().unwrap().starts_with("```") {
                lines.pop();
            }
            return (lines.join("\n"), true);
        }
        (code.to_string(), false)
    }
}

struct FixCopyNonoverlapping;
impl CodePass for FixCopyNonoverlapping {
    fn run(&self, code: &str, _ctx: &NativeContext) -> (String, bool) {
        if code.contains("copy_nonoverlapping::<T>") {
            let fixed = code.replace(
                "copy_nonoverlapping::<T>",
                "copy_nonoverlapping",
            );
            return (fixed, true);
        }
        (code.to_string(), false)
    }
}

struct RemoveStrayZeroBeforeReturn;
impl CodePass for RemoveStrayZeroBeforeReturn {
    fn run(&self, code: &str, _ctx: &NativeContext) -> (String, bool) {
        if code.contains(") .0") || code.contains(").0;") || code.contains(").0)") {
            let mut out = code.to_string();
            out = out.replace(").0)", "))");
            out = out.replace(").0;", ");");
            out = out.replace(") .0", ")");
            return (out, true);
        }
        (code.to_string(), false)
    }
}
