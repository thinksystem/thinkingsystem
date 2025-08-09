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

use super::compiler::JitCompiler;
use std::collections::HashMap;



pub type JittedFunction = unsafe extern "C" fn(*mut u64, *mut i64, *const u8) -> i64;



pub type JittedFunctionWithStack = unsafe extern "C" fn(
    *mut u64,
    *mut i64,
    *const u8,
    *const i64,
    u64,
    *mut i64,
    u64,
    *mut u64,
) -> i64;

pub struct JitCache {
    cache: HashMap<String, JittedFunction>,
    compiler: Option<JitCompiler>,
}

unsafe impl Send for JitCache {}
unsafe impl Sync for JitCache {}

impl Default for JitCache {
    fn default() -> Self {
        Self::new()
    }
}

impl JitCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            compiler: JitCompiler::new().ok(),
        }
    }

    pub fn get(&self, hash: &str) -> Option<JittedFunction> {
        self.cache.get(hash).copied()
    }

    pub fn insert(&mut self, hash: String, function: JittedFunction) {
        self.cache.insert(hash, function);
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }

    pub fn get_or_compile(
        &mut self,
        bytecode_hash: u64,
        bytecode: &[u8],
    ) -> Option<JittedFunction> {
        let hash = bytecode_hash.to_string();
        if let Some(cached) = self.cache.get(&hash) {
            return Some(*cached);
        }

        if let Some(ref mut compiler) = self.compiler {
            let name = format!("jit_func_{bytecode_hash}");
            if let Ok(compiled) = compiler.compile_with_ffi(bytecode, &name, None) {
                self.cache.insert(hash, compiled);
                return Some(compiled);
            }
        }
        None
    }
}
