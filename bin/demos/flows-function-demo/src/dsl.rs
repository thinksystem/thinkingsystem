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

#[derive(Debug, Clone)]
pub struct DslEvaluator {
    #[allow(dead_code)] 
    pub source: String,
    rules: Vec<Rule>,
    max_trail: usize,
}

#[derive(Debug, Clone)]
struct Rule {
    cond: Cond,
    actions: Actions,
}

#[derive(Debug, Clone)]
enum Actions {
    Terminate,
    Ops(Vec<Op>),
}

#[derive(Debug, Clone)]
enum Cond {
    ModEq {
        modulus: u64,
        equals: u64,
    },
    NotModEq {
        modulus: u64,
        not_equals: u64,
    },
    EqConst {
        value: u64,
    },
    Rel {
        op: RelOp,
        value: u64,
    },
    ModAndRel {
        modulus: u64,
        equals: u64,
        op: RelOp,
        value: u64,
    },
    All(Vec<Cond>), 
    Any(Vec<Cond>), 
}

#[derive(Debug, Clone, Copy)]
enum RelOp {
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone)]
enum Op {
    Div(u64),
    Mul(u64),
    Add(u64),
    Sub(u64),
}

impl DslEvaluator {
    pub fn parse(source: &str, max_trail: usize) -> anyhow::Result<Self> {
        let mut rules = Vec::new();
        let mut errors: Vec<String> = Vec::new();
        for (lineno, raw) in source.lines().enumerate() {
            let line = raw.split('#').next().unwrap().trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with("score ") {
                continue;
            }
            if !line.starts_with("rule ") {
                errors.push(format!("line {}: expected 'rule'", lineno + 1));
                continue;
            }
            let body = &line[5..];
            let mut parts = body.split("->");
            let left = parts.next().unwrap_or("").trim();
            let right = parts.next().unwrap_or("").trim();
            if left.is_empty() {
                errors.push(format!("line {}: missing condition", lineno + 1));
                continue;
            }
            if right.is_empty() {
                errors.push(format!("line {}: missing actions", lineno + 1));
                continue;
            }
            if parts.next().is_some() {
                errors.push(format!("line {}: multiple '->'", lineno + 1));
                continue;
            }
            match parse_cond(left) {
                Ok(cond) => {
                    let actions = if right.eq_ignore_ascii_case("terminate") {
                        Actions::Terminate
                    } else {
                        let mut collected = Vec::new();
                        for op_chunk in right.split(';') {
                            let op_line = op_chunk.trim();
                            if op_line.is_empty() {
                                continue;
                            }
                            match parse_ops(op_line) {
                                Ok(mut ops) => collected.append(&mut ops),
                                Err(e) => {
                                    errors.push(format!("line {}: {e}", lineno + 1));
                                    continue;
                                }
                            }
                        }
                        if collected.is_empty() {
                            errors.push(format!("line {}: no ops parsed", lineno + 1));
                            continue;
                        }
                        Actions::Ops(collected)
                    };
                    rules.push(Rule { cond, actions });
                }
                Err(e) => errors.push(format!("line {}: {e}", lineno + 1)),
            }
        }
        if !errors.is_empty() {
            return Err(anyhow::anyhow!(format!(
                "DSL_PARSE_ERRORS: {}",
                errors.join(" | ")
            )));
        }
        if rules.is_empty() {
            return Err(anyhow::anyhow!("no rules parsed"));
        }
        Ok(Self {
            source: source.to_string(),
            rules,
            max_trail: max_trail.max(1),
        })
    }
}

fn parse_cond(s: &str) -> Result<Cond, String> {
    
    let mut norm = s.replace("&&", " and ").replace("AND", " and ");
    norm = norm.replace("||", " or ").replace("OR", " or ");

    
    fn parse_expr(tokens: &[String]) -> Result<(Cond, usize), String> {
        parse_or(tokens)
    }
    fn parse_or(tokens: &[String]) -> Result<(Cond, usize), String> {
    let (left, mut idx) = parse_and(tokens)?;
        let mut ors = Vec::new();
        ors.push(left);
        while idx < tokens.len() && tokens[idx] == "or" {
            idx += 1;
            let (rhs, nidx) = parse_and(&tokens[idx..])?;
            ors.push(rhs);
            idx += nidx;
        }
        if ors.len() == 1 {
            Ok((ors.pop().unwrap(), idx))
        } else {
            Ok((Cond::Any(ors), idx))
        }
    }
    fn parse_and(tokens: &[String]) -> Result<(Cond, usize), String> {
    let (left, mut idx) = parse_primary(tokens)?;
        let mut ands = Vec::new();
        ands.push(left);
        while idx < tokens.len() && tokens[idx] == "and" {
            idx += 1;
            let (rhs, nidx) = parse_primary(&tokens[idx..])?;
            ands.push(rhs);
            idx += nidx;
        }
        if ands.len() == 1 {
            Ok((ands.pop().unwrap(), idx))
        } else {
            Ok((Cond::All(ands), idx))
        }
    }
    fn parse_primary(tokens: &[String]) -> Result<(Cond, usize), String> {
        if tokens.is_empty() {
            return Err("empty condition".into());
        }
        if tokens[0] == "(" {
            let (inner, used) = parse_or(&tokens[1..])?; 
            let end = 1 + used;
            if end >= tokens.len() || tokens[end] != ")" {
                return Err("unclosed '('".into());
            }
            return Ok((inner, end + 1));
        }
        
        let mut i = 0;
        let mut buf = Vec::new();
        while i < tokens.len() {
            let t = &tokens[i];
            if matches!(t.as_str(), "and" | "or" | ")" | "(") {
                break;
            }
            buf.push(t.clone());
            i += 1;
        }
        if buf.is_empty() {
            return Err("unexpected token".into());
        }
        let atom_str = buf.join(" ");
        let atom = parse_simple_atom(&atom_str)?;
        Ok((atom, i))
    }
    fn parse_simple_atom(seg: &str) -> Result<Cond, String> {
        let protected = seg
            .replace(">=", " __GE__ ")
            .replace("<=", " __LE__ ")
            .replace("==", " == ")
            .replace('>', " > ")
            .replace('<', " < ")
            .replace('%', " % ")
            .replace("__GE__", ">=")
            .replace("__LE__", "<=");
        let parts: Vec<&str> = protected.split_whitespace().collect();
        if parts.is_empty() {
            return Err("empty atom".into());
        }
        
        if parts.len() == 3 && parts[0] == "n" && parts[1] == "==" {
            let value: u64 = parts[2].parse().map_err(|_| "invalid const".to_string())?;
            return Ok(Cond::EqConst { value });
        }
        
        if parts.len() == 3 && parts[0] == "n" {
            let op_tok = parts[1];
            let val_tok = parts[2];
            let rel = match op_tok {
                ">" => Some(RelOp::Gt),
                "<" => Some(RelOp::Lt),
                ">=" => Some(RelOp::Ge),
                "<=" => Some(RelOp::Le),
                _ => None,
            };
            if let Some(r) = rel {
                let value: u64 = val_tok
                    .parse()
                    .map_err(|_| "invalid rel const".to_string())?;
                return Ok(Cond::Rel { op: r, value });
            }
        }
        
        if parts.len() == 5 && parts[0] == "n" && parts[1] == "%" && parts[3] == "==" {
            let modulus: u64 = parts[2]
                .parse()
                .map_err(|_| "invalid modulus".to_string())?;
            let equals: u64 = parts[4].parse().map_err(|_| "invalid equals".to_string())?;
            if modulus == 0 {
                return Err("modulus zero".into());
            }
            return Ok(Cond::ModEq { modulus, equals });
        }
        
        if parts.len() == 5 && parts[0] == "n" && parts[1] == "%" && parts[3] == "!=" {
            let modulus: u64 = parts[2]
                .parse()
                .map_err(|_| "invalid modulus".to_string())?;
            let not_equals: u64 = parts[4]
                .parse()
                .map_err(|_| "invalid not-equals".to_string())?;
            if modulus == 0 {
                return Err("modulus zero".into());
            }
            return Ok(Cond::NotModEq {
                modulus,
                not_equals,
            });
        }
        Err("unsupported condition".into())
    }
    
    let mut tokens: Vec<String> = Vec::new();
    let mut cur = String::new();
    for ch in norm.chars() {
        match ch {
            '(' | ')' => {
                if !cur.trim().is_empty() {
                    tokens.extend(cur.split_whitespace().map(|s| s.to_string()));
                }
                cur.clear();
                tokens.push(ch.to_string());
            }
            ' ' | '\t' => {
                if !cur.is_empty() && cur.ends_with(' ') {
                    continue;
                }
                cur.push(' ');
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        tokens.extend(cur.split_whitespace().map(|s| s.to_string()));
    }
    
    
    for t in tokens.iter_mut() {
        if t.eq_ignore_ascii_case("and") {
            *t = "and".into();
        } else if t.eq_ignore_ascii_case("or") {
            *t = "or".into();
        }
    }
    let (cond, used) = parse_expr(&tokens)?;
    if used != tokens.len() {
        return Err("trailing tokens".into());
    }
    
    fn optimize(c: Cond) -> Cond {
        match c {
            Cond::All(xs) => {
                if xs.len() == 2 {
                    if let (Cond::ModEq { modulus, equals }, Cond::Rel { op, value }) =
                        (&xs[0], &xs[1])
                    {
                        return Cond::ModAndRel {
                            modulus: *modulus,
                            equals: *equals,
                            op: *op,
                            value: *value,
                        };
                    }
                    if let (Cond::Rel { op, value }, Cond::ModEq { modulus, equals }) =
                        (&xs[0], &xs[1])
                    {
                        return Cond::ModAndRel {
                            modulus: *modulus,
                            equals: *equals,
                            op: *op,
                            value: *value,
                        };
                    }
                }
                Cond::All(xs.into_iter().map(optimize).collect())
            }
            Cond::Any(xs) => Cond::Any(xs.into_iter().map(optimize).collect()),
            other => other,
        }
    }
    Ok(optimize(cond))
}

fn parse_ops(line: &str) -> Result<Vec<Op>, String> {
    
    
    
    let clean = line
        .replace('=', " = ")
        .replace('*', " * ")
        .replace('+', " + ")
        .replace('-', " - ")
        .replace('/', " / ");
    let toks: Vec<&str> = clean.split_whitespace().collect();
    if toks.len() < 5 || toks[0] != "n" || toks[1] != "=" {
        return Err("unsupported op".into());
    }
    let mut ops = Vec::new();
    let mut i: usize;
    if toks[2] == "n" {
        
        if (toks.len() - 3) % 2 != 0 {
            return Err("malformed op chain".into());
        }
        i = 3;
    } else {
        
        
        if toks.len() < 5 {
            return Err("unsupported op".into());
        }
        let c = toks[2]
            .parse::<u64>()
            .map_err(|_| "invalid op value".to_string())?;
        let op_tok = toks.get(3).ok_or_else(|| "missing operator".to_string())?;
        if toks.get(4) != Some(&"n") {
            return Err("unsupported op".into());
        }
        
        let first_op = match *op_tok {
            "*" => Op::Mul(c),
            "/" => Op::Div(c),
            "+" => Op::Add(c), 
            "-" => Op::Sub(c),
            _ => return Err("bad operator".into()),
        };
        ops.push(first_op);
        
        i = 5;
        if (toks.len() - i) % 2 != 0 {
            return Err("malformed op chain".into());
        }
    }
    while i < toks.len() {
        let op_tok = toks[i];
        let val_tok = toks.get(i + 1).ok_or_else(|| "missing value".to_string())?;
        let val: u64 = val_tok
            .parse()
            .map_err(|_| "invalid op value".to_string())?;
        let op = match op_tok {
            "/" => Op::Div(val),
            "*" => Op::Mul(val),
            "+" => Op::Add(val),
            "-" => Op::Sub(val),
            _ => return Err("bad operator".into()),
        };
        ops.push(op);
        i += 2;
    }
    if ops.is_empty() {
        return Err("no ops".into());
    }
    Ok(ops)
}

impl EvalFn for DslEvaluator {
    fn eval(&self, n: u64, memo: &dyn MemoBackend) -> EvalOutcome {
        if let Some(v) = memo.get(n) { return EvalOutcome::new(v, Vec::new(), Some(n)); }
        let mut x = n;
        let mut trail: Vec<u64> = Vec::new();
        let mut peak = n;
        while trail.len() < self.max_trail {
            if let Some(v) = memo.get(x) {
                
                let base = v;
                let total = base + trail.len() as u32;
                let mut path: Vec<(u64, u32)> = Vec::with_capacity(trail.len());
                for (i, vn) in trail.iter().enumerate() {
                    path.push((*vn, total - i as u32));
                }
                return EvalOutcome::new(total, path, Some(peak));
            }
            
            if x == 1 {
                break;
            }
            let mut applied = false;
            for rule in &self.rules {
                if rule.matches(x) {
                    match &rule.actions {
                        Actions::Terminate => {
                            applied = true;
                            trail.push(x);
                            x = 1;
                        }
                        Actions::Ops(ops) => {
                            let mut nx = x;
                            for op in ops {
                                match op {
                                    Op::Div(v) => {
                                        if *v == 0 {
                                            return EvalOutcome::new(0, Vec::new(), Some(peak));
                                        }
                                        nx /= *v;
                                    }
                                    Op::Mul(v) => {
                                        if let Some(tmp) = nx.checked_mul(*v) {
                                            nx = tmp;
                                        } else {
                                            return finalize_with_base(trail, 0, peak);
                                        }
                                    }
                                    Op::Add(v) => {
                                        if let Some(tmp) = nx.checked_add(*v) {
                                            nx = tmp;
                                        } else {
                                            return finalize_with_base(trail, 0, peak);
                                        }
                                    }
                                    Op::Sub(v) => {
                                        nx = nx.saturating_sub(*v);
                                    }
                                }
                            }
                            if nx == x {
                                
                                return finalize_with_lookup(&trail, x, memo, peak);
                            }
                            trail.push(x);
                            if nx > peak { peak = nx; }
                            x = nx;
                            applied = true;
                        }
                    }
                    break;
                }
            }
            if !applied {
                break;
            }
            if x == 1 {
                break;
            }
        }
        
        let base = if x == 1 { 1 } else { memo.get(x).unwrap_or(1) };
        let total = base + trail.len() as u32;
        if trail.is_empty() {
            return EvalOutcome::new(total, Vec::new(), Some(peak));
        }
        let mut path: Vec<(u64, u32)> = Vec::with_capacity(trail.len());
        for (i, vn) in trail.iter().enumerate() {
            path.push((*vn, total - i as u32));
        }
        EvalOutcome::new(total, path, Some(peak))
    }
}

fn finalize_with_lookup(trail: &[u64], x: u64, memo: &dyn MemoBackend, peak: u64) -> EvalOutcome {
    let base = memo.get(x).unwrap_or(1);
    finalize_with_base(trail.to_vec(), base, peak)
}
fn finalize_with_base(trail: Vec<u64>, base: u32, peak: u64) -> EvalOutcome {
    let total = base + trail.len() as u32;
    if trail.is_empty() { return EvalOutcome::new(total, Vec::new(), Some(peak)); }
    let mut path: Vec<(u64,u32)> = Vec::with_capacity(trail.len());
    for (i,vn) in trail.iter().enumerate() { path.push((*vn, total - i as u32)); }
    EvalOutcome::new(total, path, Some(peak))
}

impl Rule {
    fn matches(&self, n: u64) -> bool {
        self.cond.matches(n)
    }
}
impl Cond {
    fn matches(&self, n: u64) -> bool {
        match self {
            Cond::ModEq { modulus, equals } => n % *modulus == *equals,
            Cond::NotModEq {
                modulus,
                not_equals,
            } => n % *modulus != *not_equals,
            Cond::EqConst { value } => n == *value,
            Cond::Rel { op, value } => rel_cmp(n, *op, *value),
            Cond::ModAndRel {
                modulus,
                equals,
                op,
                value,
            } => (n % *modulus == *equals) && rel_cmp(n, *op, *value),
            Cond::All(list) => list.iter().all(|c| c.matches(n)),
            Cond::Any(list) => list.iter().any(|c| c.matches(n)),
        }
    }
}

fn rel_cmp(n: u64, op: RelOp, v: u64) -> bool {
    match op {
        RelOp::Lt => n < v,
        RelOp::Le => n <= v,
        RelOp::Gt => n > v,
        RelOp::Ge => n >= v,
    }
}
