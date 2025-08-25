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



use stele::flows::dynamic_executor::strategy::{EvalFn, MemoBackend, EvalOutcome};




#[derive(Default)]
pub struct ParityRuleEvaluator {
    pub div_even: u64,
    pub mul_odd: u64,
    pub add_odd: u64,
    pub max_trail: usize,
}

impl ParityRuleEvaluator {
    #[allow(dead_code)] 
    pub fn new(div_even: u64, mul_odd: u64, add_odd: u64, max_trail: usize) -> Self {
        Self { div_even, mul_odd, add_odd, max_trail }
    }
}

impl EvalFn for ParityRuleEvaluator {
    fn eval(&self, n: u64, memo: &dyn MemoBackend) -> EvalOutcome {
        let mut x = n;
        let mut trail: Vec<u64> = Vec::new();
        let max_trail = self.max_trail.max(1);
        while memo.get(x).is_none() {
            trail.push(x);
            if x % self.div_even == 0 {
                x /= self.div_even;
            } else {
                match x.checked_mul(self.mul_odd).and_then(|v| v.checked_add(self.add_odd)) {
                    Some(nx) => x = nx,
                    None => break,
                }
            }
            if trail.len() > max_trail { break; }
        }
        let base = memo.get(x).unwrap_or(1);
        let total = base + trail.len() as u32;
        if trail.is_empty() { return EvalOutcome::new(total, Vec::new(), Some(n)); }
        let mut path: Vec<(u64,u32)> = Vec::with_capacity(trail.len());
        for (i,v) in trail.iter().enumerate() { path.push((*v, total - i as u32)); }
        EvalOutcome::new(total, path, Some(n))
    }
}
