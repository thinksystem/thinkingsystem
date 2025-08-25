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
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;


#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Metric {
    pub primary: u64, 
    pub score: u32,   
}


pub trait MemoBackend: Send + Sync {
    fn get(&self, key: u64) -> Option<u32>;
    fn insert_path(&self, path: &[(u64, u32)]);
}


pub struct DenseMemo {
    data: UnsafeCell<Vec<u32>>,
}
impl DenseMemo {
    pub fn new(limit: usize) -> Self {
        let mut d = vec![0; limit + 1];
        if limit >= 1 {
            d[1] = 1;
        }
        Self {
            data: UnsafeCell::new(d),
        }
    }
}

unsafe impl Send for DenseMemo {}
unsafe impl Sync for DenseMemo {}
impl MemoBackend for DenseMemo {
    fn get(&self, key: u64) -> Option<u32> {
        let k = key as usize;
        unsafe { (&*self.data.get()).get(k).copied().filter(|v| *v > 0) }
    }
    fn insert_path(&self, path: &[(u64, u32)]) {
        unsafe {
            let data = &mut *self.data.get();
            let len = data.len();
            for (k, v) in path {
                let idx = *k as usize;
                if idx < len && data[idx] == 0 {
                    data[idx] = *v;
                }
            }
        }
    }
}


pub struct ShardedHashMemo {
    shards: Vec<Mutex<HashMap<u64, u32>>>,
    mask: u64,
}
impl ShardedHashMemo {
    pub fn new(shards_pow2: usize) -> Self {
        let shards = (0..shards_pow2)
            .map(|_| Mutex::new(HashMap::new()))
            .collect::<Vec<_>>();
        let mask = (shards_pow2 as u64) - 1;
        let m = Self { shards, mask };
        m.seed();
        m
    }
    fn seed(&self) {
        self.insert_path(&[(1, 1)])
    }
    fn shard(&self, k: u64) -> usize {
        (k & self.mask) as usize
    }
}
impl MemoBackend for ShardedHashMemo {
    fn get(&self, key: u64) -> Option<u32> {
        self.shards[self.shard(key)].lock().ok()?.get(&key).copied()
    }
    fn insert_path(&self, path: &[(u64, u32)]) {
        
        let mut buckets: Vec<Vec<(u64, u32)>> = vec![Vec::new(); self.shards.len()];
        for (k, v) in path {
            buckets[self.shard(*k)].push((*k, *v));
        }
        for (i, b) in buckets.into_iter().enumerate() {
            if b.is_empty() {
                continue;
            }
            if let Ok(mut g) = self.shards[i].lock() {
                for (k, v) in b {
                    g.entry(k).or_insert(v);
                }
            }
        }
    }
}


#[derive(Clone)]
pub struct StrategyPlan {
    
    pub range_start: u64,
    pub range_end: u64,
    pub prefer_dense_cutoff: u64, 
    pub shards: usize,            
    pub chunk: u64,               
    pub odd_only: bool,           
    
    pub progress_log_interval: u64,
    
    pub early_stop_no_improve: Option<u64>,
    
    pub upper_bound: Option<Arc<dyn UpperBoundEstimator>>,
    
    pub top_k: Option<usize>,
    
    pub memory_limit_mb: Option<u64>,
    
    pub min_score: Option<u32>,
    
    pub min_aux: Option<u64>,
    
    pub custom_score_expr: Option<String>,
}

impl Default for StrategyPlan {
    fn default() -> Self {
        Self {
            range_start: 2,
            range_end: 1_000_000,
            prefer_dense_cutoff: 120_000_000,
            shards: 64,
            chunk: 1_000_000,
            odd_only: false,
            progress_log_interval: 0,
            early_stop_no_improve: None,
            upper_bound: None,
            top_k: None,
            memory_limit_mb: None,
            min_score: None,
            min_aux: None,
            custom_score_expr: None,
        }
    }
}


pub struct EvalOutcome {
    pub score: u32,
    pub path: Vec<(u64, u32)>,
    pub aux: Option<u64>,
}
impl EvalOutcome {
    pub fn new(score: u32, path: Vec<(u64, u32)>, aux: Option<u64>) -> Self {
        Self { score, path, aux }
    }
}

pub trait EvalFn: Send + Sync {
    fn eval(&self, n: u64, memo: &dyn MemoBackend) -> EvalOutcome;
}


pub trait UpperBoundEstimator: Send + Sync {
    
    fn max_remaining_score(
        &self,
        next_numeric: u64,
        end_numeric: u64,
        best_score: u32,
    ) -> Option<u32>;
}


#[derive(Debug)]
pub struct StrategyResult {
    pub best_n: u64,
    pub best_score: u32,
    pub top: Option<Vec<TopEntry>>, 
    pub pareto: Option<Vec<ParetoEntry>>, 
}

type TopEntry = (u64, u32, Option<u64>, f64);
type ParetoEntry = (u64, u32, Option<u64>);

fn update_pareto(pf: &mut Vec<ParetoEntry>, n: u64, score: u32, aux: Option<u64>) {
    
    
    let mut dominated_idx: Vec<usize> = Vec::new();
    let mut dominated = false;
    for (i, (en, es, ea)) in pf.iter().enumerate() {
        let ea_v = ea.unwrap_or(0);
        let aux_v = aux.unwrap_or(0);
        if *es >= score && ea_v >= aux_v && (*es > score || ea_v > aux_v) {
            dominated = true;
            break;
        }
        if score >= *es && aux_v >= ea_v && (score > *es || aux_v > ea_v) {
            dominated_idx.push(i);
        }
        if *es == score && ea_v == aux_v && *en == n {
            dominated = true;
            break;
        }
    }
    if dominated {
        return;
    }
    
    for i in dominated_idx.into_iter().rev() {
        pf.remove(i);
    }
    pf.push((n, score, aux));
}


pub fn execute(plan: &StrategyPlan, eval: &Arc<dyn EvalFn>) -> Result<StrategyResult> {
    let mut use_dense = plan.range_end <= plan.prefer_dense_cutoff;
    let decision_reason;
    if use_dense {
        if let Some(limit_mb) = plan.memory_limit_mb {
            
            let bytes = (plan.range_end as u128 + 1) * 4u128;
            let mb = bytes / (1024 * 1024) as u128;
            if mb > limit_mb as u128 {
                decision_reason = format!("dense_est={mb}MB>limit={limit_mb}MB");
                use_dense = false;
            } else {
                decision_reason = format!("dense_est={mb}MB<=limit={limit_mb}MB");
            }
        } else {
            decision_reason = "no_memory_limit".into();
        }
    } else {
        decision_reason = format!(
            "range_end={} > cutoff={}",
            plan.range_end, plan.prefer_dense_cutoff
        );
    }
    if use_dense {
        println!(
            "EXECUTION_STRATEGY using=DENSE reason={} range_end={} cutoff={} memory_limit_mb={:?}",
            decision_reason, plan.range_end, plan.prefer_dense_cutoff, plan.memory_limit_mb
        );
        execute_dense(plan, eval.as_ref())
    } else {
        println!(
            "EXECUTION_STRATEGY using=SPARSE_HASH reason={} range_end={} shards={} chunk={} memory_limit_mb={:?}",
            decision_reason, plan.range_end, plan.shards, plan.chunk, plan.memory_limit_mb
        );
        execute_sparse(plan, Arc::clone(eval))
    }
}


#[derive(Clone, Debug)]
enum ExprToken {
    Num(f64),
    Score,
    Aux,
    Laux, 
    Op(char),
}

fn compile_expr(expr: &str) -> Option<Vec<ExprToken>> {
    
    let mut output: Vec<ExprToken> = Vec::new();
    let mut ops: Vec<char> = Vec::new();
    let mut i = 0;
    let bytes = expr.as_bytes();
    let prec = |c: char| match c {
        '+' | '-' => 1,
        '*' | '/' => 2,
        _ => 0,
    };
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            let start = i;
            let mut j = i + 1;
            while j < bytes.len() {
                let cj = bytes[j] as char;
                if cj.is_ascii_digit() || cj == '.' {
                    j += 1;
                } else {
                    break;
                }
            }
            if let Ok(v) = expr[start..j].parse::<f64>() {
                output.push(ExprToken::Num(v));
            }
            i = j;
            continue;
        }
        if c.is_ascii_alphabetic() {
            
            let start = i;
            let mut j = i + 1;
            while j < bytes.len() {
                let cj = bytes[j] as char;
                if cj.is_ascii_alphanumeric() || cj == '_' {
                    j += 1;
                } else {
                    break;
                }
            }
            let ident = &expr[start..j];
            match ident {
                "score" => output.push(ExprToken::Score),
                "aux" => output.push(ExprToken::Aux),
                "laux" => output.push(ExprToken::Laux),
                _ => return None,
            }
            i = j;
            continue;
        }
        match c {
            '+' | '-' | '*' | '/' => {
                while let Some(top) = ops.last().copied() {
                    if prec(top) >= prec(c) {
                        output.push(ExprToken::Op(ops.pop().unwrap()));
                    } else {
                        break;
                    }
                }
                ops.push(c);
            }
            '(' => ops.push(c),
            ')' => {
                while let Some(top) = ops.pop() {
                    if top == '(' {
                        break;
                    }
                    output.push(ExprToken::Op(top));
                }
            }
            _ => return None,
        }
        i += 1;
    }
    while let Some(op) = ops.pop() {
        if op == '(' {
            return None;
        }
        output.push(ExprToken::Op(op));
    }
    Some(output)
}

fn eval_expr(rpn: &[ExprToken], score: u32, aux: Option<u64>) -> Option<f64> {
    let mut stack: Vec<f64> = Vec::new();
    for t in rpn {
        match t {
            ExprToken::Num(v) => stack.push(*v),
            ExprToken::Score => stack.push(score as f64),
            ExprToken::Aux => stack.push(aux.unwrap_or(0) as f64),
            ExprToken::Laux => {
                let v = aux.unwrap_or(0);
                let lv = if v == 0 {
                    0.0
                } else {
                    (v as f64 + 1.0).log10()
                };
                stack.push(lv);
            }
            ExprToken::Op(op) => {
                if stack.len() < 2 {
                    return None;
                }
                let b = stack.pop().unwrap();
                let a = stack.pop().unwrap();
                let v = match op {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    '/' => {
                        if b == 0.0 {
                            return None;
                        } else {
                            a / b
                        }
                    }
                    _ => return None,
                };
                stack.push(v);
            }
        }
    }
    if stack.len() == 1 {
        Some(stack[0])
    } else {
        None
    }
}

fn execute_dense(plan: &StrategyPlan, eval: &dyn EvalFn) -> Result<StrategyResult> {
    let t_start = Instant::now();
    let limit = plan.range_end as usize;
    let memo = DenseMemo::new(limit);
    let mut best = StrategyResult {
        best_n: 1,
        best_score: 1,
        top: plan.top_k.map(|_| Vec::new()),
        pareto: None,
    };
    let compiled_expr = plan
        .custom_score_expr
        .as_ref()
        .and_then(|e| compile_expr(e));
    let start_n = if plan.odd_only {
        let s = plan.range_start.max(3);
        if s % 2 == 0 {
            s + 1
        } else {
            s
        }
    } else {
        plan.range_start.max(2)
    };
    let step = if plan.odd_only { 2 } else { 1 };
    let mut processed: u64 = 0;
    let total_candidates: u64 = {
        let last = plan.range_end;
        if last < start_n {
            0
        } else if plan.odd_only {
            ((last - start_n) / 2) + 1
        } else {
            (last - start_n) + 1
        }
    };
    let mut since_improve: u64 = 0;
    
    let mut last_check = Instant::now();
    let mut last_processed = 0u64;
    let mut dynamic_log_interval = plan.progress_log_interval.max(1);
    for n in (start_n..=plan.range_end).step_by(step as usize) {
        let EvalOutcome { score, path, aux } = eval.eval(n, &memo);
        if let Some(ms) = plan.min_score {
            if score < ms {
                continue;
            }
        }
        if let Some(ma) = plan.min_aux {
            if aux.unwrap_or(0) < ma {
                continue;
            }
        }
        if score > best.best_score {
            best.best_score = score;
            best.best_n = n;
            since_improve = 0;
        } else {
            since_improve += 1;
        }
        
        if aux.is_some() {
            if best.pareto.is_none() {
                best.pareto = Some(Vec::new());
            }
            if let Some(ref mut pf) = best.pareto {
                update_pareto(pf, n, score, aux);
            }
        }
        
        if let Some(k) = plan.top_k {
            if let Some(ref mut tv) = best.top {
                let order_score = if let Some(rpn) = &compiled_expr {
                    eval_expr(rpn, score, aux).unwrap_or(score as f64)
                } else {
                    score as f64
                };
                tv.push((n, score, aux, order_score));
                if tv.len() > k * 6 {
                    
                    tv.sort_by(|a, b| {
                        b.3.partial_cmp(&a.3)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| b.1.cmp(&a.1))
                            .then_with(|| b.0.cmp(&a.0))
                    });
                    tv.truncate(k);
                }
            }
        }
        memo.insert_path(&path);
        processed += 1;
        if dynamic_log_interval > 0 && processed % dynamic_log_interval == 0 {
            let numeric_pct = (n as f64 / plan.range_end as f64) * 100.0;
            let candidate_pct = (processed as f64 / total_candidates.max(1) as f64) * 100.0;
            println!("STRAT_PROGRESS numeric_pct={:.2}% candidate_pct={:.2}% processed={} total={} best_n={} best_score={} log_interval={}", numeric_pct, candidate_pct, processed, total_candidates, best.best_n, best.best_score, dynamic_log_interval);
            
            let now = Instant::now();
            let elapsed = now.duration_since(last_check).as_secs_f64();
            if elapsed > 0.2 {
                
                let delta = processed - last_processed;
                let tput = delta as f64 / elapsed;
                
                if tput > 800_000.0 && dynamic_log_interval < plan.progress_log_interval * 8 {
                    dynamic_log_interval =
                        (dynamic_log_interval * 2).min(plan.progress_log_interval * 8);
                    println!("ADAPT log_interval_increase new={dynamic_log_interval} tput={tput:.0}");
                } else if tput < 200_000.0 && dynamic_log_interval > 1 {
                    dynamic_log_interval = (dynamic_log_interval / 2).max(1);
                    println!("ADAPT log_interval_decrease new={dynamic_log_interval} tput={tput:.0}");
                }
                last_processed = processed;
                last_check = now;
            }
        }
        if let Some(window) = plan.early_stop_no_improve {
            if window > 0 && since_improve >= window {
                break;
            }
        }
        if let Some(ub) = &plan.upper_bound {
            if let Some(rem_max) =
                ub.max_remaining_score(n + step as u64, plan.range_end, best.best_score)
            {
                if rem_max <= best.best_score {
                    break;
                }
            }
        }
    }
    
    if let (Some(k), Some(ref mut tv)) = (plan.top_k, &mut best.top) {
        tv.sort_by(|a, b| {
            b.3.partial_cmp(&a.3)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| b.0.cmp(&a.0))
        });
        tv.truncate(k);
    }
    let elapsed = t_start.elapsed().as_secs_f64();
    if elapsed > 0.0 {
        println!("EXECUTION_STATS mode=DENSE processed={} elapsed_sec={:.3} throughput={:.0} best_n={} best_score={}", processed, elapsed, (processed as f64/elapsed), best.best_n, best.best_score);
    }
    Ok(best)
}

fn execute_sparse(plan: &StrategyPlan, eval: Arc<dyn EvalFn>) -> Result<StrategyResult> {
    let t_start = Instant::now();
    use std::thread;
    let memo = Arc::new(ShardedHashMemo::new(plan.shards));
    let best_n = Arc::new(AtomicU64::new(1));
    let best_score = Arc::new(AtomicU32::new(1));
    let top_store: Option<Arc<Mutex<Vec<TopEntry>>>> =
        plan.top_k.map(|_| Arc::new(Mutex::new(Vec::new())));
    let pareto_store: Arc<Mutex<Option<Vec<ParetoEntry>>>> = Arc::new(Mutex::new(None));
    let compiled_expr = plan
        .custom_score_expr
        .as_ref()
        .and_then(|e| compile_expr(e));
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
        .min(64);
    let initial_start = if plan.odd_only {
        let s = plan.range_start.max(3);
        if s % 2 == 0 {
            s + 1
        } else {
            s
        }
    } else {
        plan.range_start.max(2)
    };
    let next = Arc::new(AtomicU64::new(initial_start));
    
    let adaptive_chunk = Arc::new(AtomicU64::new(plan.chunk));
    let end = plan.range_end;
    let processed = Arc::new(AtomicU64::new(0));
    let last_improve_at = Arc::new(AtomicU64::new(0));
    let stop_flag = Arc::new(AtomicBool::new(false));
    
    let total_candidates: u64 = {
        let start = initial_start;
        let step = if plan.odd_only { 2 } else { 1 };
        if end < start {
            0
        } else {
            ((end - start) / step) + 1
        }
    };
    let mut handles = Vec::new();
    for _ in 0..threads {
        let memo_c = Arc::clone(&memo);
        let best_n_c = Arc::clone(&best_n);
        let best_score_c = Arc::clone(&best_score);
        let next_c = Arc::clone(&next);
        let plan_c = plan.clone();
        let eval_c = Arc::clone(&eval);
        let processed_c = Arc::clone(&processed);
        let last_improve_c = Arc::clone(&last_improve_at);
        let stop_c = Arc::clone(&stop_flag);
        let ub = plan.upper_bound.as_ref().map(Arc::clone);
        let top_c = top_store.as_ref().map(Arc::clone);
        let pareto_outer = Arc::clone(&pareto_store);
        let compiled_expr_c = compiled_expr.clone();
        let adaptive_chunk = Arc::clone(&adaptive_chunk);
        handles.push(thread::spawn(move || {
            loop {
                if stop_c.load(Ordering::Relaxed) { break; }
                let chunk_now = adaptive_chunk.load(Ordering::Relaxed).max(1);
                let start = next_c.fetch_add(chunk_now, Ordering::Relaxed);
                if start > end { break; }
                let stop = (start + chunk_now - 1).min(end);
                let step = if plan_c.odd_only {2} else {1};
                
                let mut n = if plan_c.odd_only { if start % 2 == 0 { start + 1 } else { start } } else { start };
                while n <= stop {
                    let EvalOutcome { score, path, aux } = eval_c.eval(n, memo_c.as_ref());
                    if let Some(ms) = plan_c.min_score { if score < ms { n += step as u64; continue; } }
                    if let Some(ma) = plan_c.min_aux { if aux.unwrap_or(0) < ma { n += step as u64; continue; } }
                    if !path.is_empty() { memo_c.insert_path(&path); }
                    let cur = best_score_c.load(Ordering::Relaxed);
                    if score > cur {
                        best_score_c.store(score, Ordering::Relaxed);
                        best_n_c.store(n, Ordering::Relaxed);
                        last_improve_c.store(processed_c.load(Ordering::Relaxed), Ordering::Relaxed);
                    }
                    
                    if aux.is_some() {
                        if let Ok(mut slot) = pareto_outer.lock() {
                            if slot.is_none() { *slot = Some(Vec::new()); }
                            if let Some(ref mut v) = *slot { update_pareto(v, n, score, aux); }
                        }
                    }
                    
                    if let Some(ref top_arc) = top_c { if let Ok(mut vec) = top_arc.lock() { let order_score = if let Some(rpn) = &compiled_expr_c { eval_expr(rpn, score, aux).unwrap_or(score as f64) } else { score as f64 }; vec.push((n, score, aux, order_score)); if let Some(k) = plan_c.top_k { if vec.len() > k * 10 { vec.sort_by(|a,b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal).then_with(|| b.1.cmp(&a.1)).then_with(|| b.0.cmp(&a.0))); vec.truncate(k); } } } }
                    let proc = processed_c.fetch_add(1, Ordering::Relaxed) + 1;
                    
                    if plan_c.progress_log_interval > 0 && proc % plan_c.progress_log_interval == 0 {
                        let numeric_pct = (n as f64 / end as f64) * 100.0;
                        let candidate_pct = (proc as f64 / total_candidates.max(1) as f64) * 100.0;
                        println!("STRAT_PROGRESS numeric_pct={:.2}% candidate_pct={:.2}% processed={} total={} best_n={} best_score={} threads={} chunk={}", numeric_pct, candidate_pct, proc, total_candidates, best_n_c.load(Ordering::Relaxed), best_score_c.load(Ordering::Relaxed), threads, chunk_now);
                    }
                    
                    if let Some(window) = plan_c.early_stop_no_improve { if window>0 { let since = proc - last_improve_c.load(Ordering::Relaxed); if since >= window { stop_c.store(true, Ordering::Relaxed); break; } } }
                    
                    if let Some(ref ub_fn) = ub { if let Some(rem_max) = ub_fn.max_remaining_score(n+step as u64, end, best_score_c.load(Ordering::Relaxed)) { if rem_max <= best_score_c.load(Ordering::Relaxed) { stop_c.store(true, Ordering::Relaxed); break; } } }
                    n += step as u64;
                }
            }
        }));
    }
    
    {
        let processed_c = Arc::clone(&processed);
        let stop_c = Arc::clone(&stop_flag);
        let adaptive_chunk_c = Arc::clone(&adaptive_chunk);
        let next_c = Arc::clone(&next);
        let end_c = end;
        let total_c = total_candidates;
        std::thread::spawn(move || {
            let mut last_check = Instant::now();
            let mut last_processed = 0u64;
            let mut stagnant_iters = 0u32;
            loop {
                if stop_c.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
                let now = Instant::now();
                let elapsed = now.duration_since(last_check).as_secs_f64();
                if elapsed < 0.05 {
                    continue;
                }
                let proc_now = processed_c.load(Ordering::Relaxed);
                let delta = proc_now - last_processed;
                let tput = (delta as f64 / elapsed).max(0.0);
                let current_chunk = adaptive_chunk_c.load(Ordering::Relaxed);
                if delta == 0 {
                    stagnant_iters += 1;
                } else {
                    stagnant_iters = 0;
                }
                if stagnant_iters >= 4 && current_chunk > 1 {
                    let new_chunk = (current_chunk / 2).max(1);
                    adaptive_chunk_c.store(new_chunk, Ordering::Relaxed);
                    println!("ADAPT chunk_reduced old={current_chunk} new={new_chunk}");
                    stagnant_iters = 0;
                } else if tput > 0.0
                    && delta >= current_chunk.saturating_sub(1)
                    && current_chunk < 1_000_000
                {
                    let new_chunk = (current_chunk * 2).min(1_000_000);
                    adaptive_chunk_c.store(new_chunk, Ordering::Relaxed);
                    println!("ADAPT chunk_increased old={current_chunk} new={new_chunk} tput={tput:.0}");
                }
                
                let next_pos = next_c.load(Ordering::Relaxed);
                if next_pos <= end_c {
                    let remaining_nums = end_c.saturating_sub(next_pos) + 1;
                    if remaining_nums < current_chunk && remaining_nums > 0 {
                        adaptive_chunk_c.store(remaining_nums, Ordering::Relaxed);
                        println!("ADAPT chunk_tail_contract old={current_chunk} new={remaining_nums} remaining={remaining_nums} next={next_pos} end={end_c}");
                    } else {
                        let progress = proc_now as f64 / total_c as f64;
                        if progress > 0.90 && current_chunk as f64 > 0.02 * total_c as f64 {
                            let reduced = (current_chunk / 2).max(1);
                            if reduced < current_chunk {
                                adaptive_chunk_c.store(reduced, Ordering::Relaxed);
                                println!("ADAPT chunk_near_end_shrink old={current_chunk} new={reduced} progress={progress:.2}");
                            }
                        }
                    }
                }
                last_processed = proc_now;
                last_check = now;
            }
        });
    }
    for h in handles {
        let _ = h.join();
    }
    let mut top_final = None;
    if let (Some(k), Some(store)) = (plan.top_k, top_store) {
        if let Ok(mut v) = store.lock() {
            v.sort_by(|a, b| {
                b.3.partial_cmp(&a.3)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| b.1.cmp(&a.1))
                    .then_with(|| b.0.cmp(&a.0))
            });
            v.truncate(k);
            top_final = Some(v.clone());
        }
    }
    let pareto_final = {
        if let Ok(slot) = pareto_store.lock() {
            slot.clone()
        } else {
            None
        }
    };
    let elapsed = t_start.elapsed().as_secs_f64();
    let processed_final = processed.load(Ordering::Relaxed);
    if elapsed > 0.0 {
        println!(
            "EXECUTION_STATS mode=SPARSE processed={} elapsed_sec={:.3} throughput={:.0} best_n={} best_score={}",
            processed_final,
            elapsed,
            processed_final as f64 / elapsed,
            best_n.load(Ordering::Relaxed),
            best_score.load(Ordering::Relaxed)
        );
    }
    Ok(StrategyResult {
        best_n: best_n.load(Ordering::Relaxed),
        best_score: best_score.load(Ordering::Relaxed),
        top: top_final,
        pareto: pareto_final,
    })
}


pub struct PlaceholderEval;
impl EvalFn for PlaceholderEval {
    fn eval(&self, _n: u64, _memo: &dyn MemoBackend) -> EvalOutcome {
        EvalOutcome::new(1, Vec::new(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct IncEval;
    impl EvalFn for IncEval {
        fn eval(&self, n: u64, _: &dyn MemoBackend) -> EvalOutcome {
            EvalOutcome::new(n as u32, Vec::new(), Some(n))
        }
    }
    #[test]
    fn dense_best() {
        let plan = StrategyPlan {
            range_end: 100,
            prefer_dense_cutoff: 500,
            progress_log_interval: 0,
            ..Default::default()
        };
        let eval: Arc<dyn EvalFn> = Arc::new(IncEval);
        let r = execute(&plan, &eval).unwrap();
        assert_eq!(r.best_n, 100);
    }
    #[test]
    fn sparse_best() {
        let plan = StrategyPlan {
            range_end: 1_000_000,
            prefer_dense_cutoff: 10,
            progress_log_interval: 0,
            ..Default::default()
        };
        let eval: Arc<dyn EvalFn> = Arc::new(IncEval);
        let r = execute(&plan, &eval).unwrap();
        assert_eq!(r.best_n, 1_000_000);
    }
}
