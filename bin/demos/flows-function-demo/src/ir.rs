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


#[derive(Debug, Clone, serde::Deserialize)]
pub struct IRFunction {
    pub params: Vec<IRParam>,
    #[serde(default, alias = "local", alias = "local_vars", alias = "temp_locals")]
    pub locals: Vec<IRLocal>,
    pub body: Vec<IRNode>,
}
#[derive(Debug, Clone, serde::Deserialize)]
pub struct IRParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}
#[derive(Debug, Clone, serde::Deserialize)]
pub struct IRLocal {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "op")]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
pub enum IRNode {
    F64_CONST {
        value: f64,
    },
    F64_ADD {
        left: Box<IRNode>,
        right: Box<IRNode>,
    },
    F64_SUB {
        left: Box<IRNode>,
        right: Box<IRNode>,
    },
    F64_MUL {
        left: Box<IRNode>,
        right: Box<IRNode>,
    },
    F64_DIV {
        left: Box<IRNode>,
        right: Box<IRNode>,
    },
    I64_CONST {
        value: i64,
    },
    I32_CONST {
        value: i32,
    },
    #[serde(alias = "LOCAL_GET")]
    GET_LOCAL {
        name: String,
    },
    #[serde(alias = "LOCAL_SET")]
    SET_LOCAL {
        #[serde(alias = "target")]
        local: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expr: Option<Box<IRNode>>,
    },
    I64_ADD {
        left: Box<IRNode>,
        right: Box<IRNode>,
    },
    I64_SUB {
        left: Box<IRNode>,
        right: Box<IRNode>,
    },
    I64_MUL {
        left: Box<IRNode>,
        right: Box<IRNode>,
    },
    I64_REM_S {
        left: Box<IRNode>,
        right: Box<IRNode>,
    },
    I64_GT_S {
        left: Box<IRNode>,
        right: Box<IRNode>,
    },
    I64_LT_S {
        left: Box<IRNode>,
        right: Box<IRNode>,
    },
    I64_EQZ {
        value: Box<IRNode>,
    },
    I32_EQ {
        left: Box<IRNode>,
        right: Box<IRNode>,
    },
    I64_TRUNC_F64_S {
        value: Box<IRNode>,
    },
    F64_CONVERT_I64_S {
        value: Box<IRNode>,
    },
    F64_SQRT {
        value: Box<IRNode>,
    },
    BLOCK {
        label: String,
        body: Vec<IRNode>,
    },
    LOOP {
        label: String,
        body: Vec<IRNode>,
    },
    BR {
        label: String,
    },
    BR_IF {
        label: String,
        cond: Box<IRNode>,
    },
    IF {
        cond: Box<IRNode>,
        then: Vec<IRNode>,
        #[serde(default)]
        else_: Vec<IRNode>,
    },
    RETURN {
        value: Box<IRNode>,
    },
}

fn ir_type_to_wat(t: &str) -> &str {
    match t {
        "f64" => "f64",
        "i64" => "i64",
        
        "i32" => "i64",
        other => other,
    }
}

fn emit_ir(node: &IRNode, out: &mut String, indent: usize) {
    let ind = |n: usize| "  ".repeat(n);
    match node {
        IRNode::F64_CONST { value } => {
            out.push_str(&format!("{}(f64.const {value})\n", ind(indent)))
        }
        IRNode::F64_ADD { left, right }
        | IRNode::F64_SUB { left, right }
        | IRNode::F64_MUL { left, right }
        | IRNode::F64_DIV { left, right } => {
            emit_ir(left, out, indent);
            emit_ir(right, out, indent);
            let op = match node {
                IRNode::F64_ADD { .. } => "f64.add",
                IRNode::F64_SUB { .. } => "f64.sub",
                IRNode::F64_MUL { .. } => "f64.mul",
                IRNode::F64_DIV { .. } => "f64.div",
                _ => unreachable!(),
            };
            out.push_str(&format!("{}({})\n", ind(indent), op));
        }
        IRNode::I64_CONST { value } => {
            out.push_str(&format!("{}(i64.const {value})\n", ind(indent)))
        }
        IRNode::I32_CONST { value } => {
            
            out.push_str(&format!("{}(i64.const {value})\n", ind(indent)))
        }
        IRNode::GET_LOCAL { name } => {
            out.push_str(&format!("{}(local.get ${})\n", ind(indent), name))
        }
        IRNode::SET_LOCAL { local, expr } => {
            if let Some(e) = expr {
                emit_ir(e, out, indent);
            }
            out.push_str(&format!("{}(local.set ${})\n", ind(indent), local));
        }
        IRNode::I64_ADD { left, right }
        | IRNode::I64_SUB { left, right }
        | IRNode::I64_MUL { left, right }
        | IRNode::I64_REM_S { left, right }
        | IRNode::I64_GT_S { left, right }
        | IRNode::I64_LT_S { left, right }
        | IRNode::I32_EQ { left, right } => {
            emit_ir(left, out, indent);
            emit_ir(right, out, indent);
            let op = match node {
                IRNode::I64_ADD { .. } => "i64.add",
                IRNode::I64_SUB { .. } => "i64.sub",
                IRNode::I64_MUL { .. } => "i64.mul",
                IRNode::I64_REM_S { .. } => "i64.rem_s",
                IRNode::I64_GT_S { .. } => "i64.gt_s",
                IRNode::I64_LT_S { .. } => "i64.lt_s",
                IRNode::I32_EQ { .. } => "i64.eq", 
                _ => "",
            };
            out.push_str(&format!("{}({})\n", ind(indent), op));
        }
        IRNode::I64_EQZ { value } => {
            emit_ir(value, out, indent);
            out.push_str(&format!("{}(i64.eqz)\n", ind(indent)));
        }
        IRNode::I64_TRUNC_F64_S { value } => {
            emit_ir(value, out, indent);
            out.push_str(&format!("{}(i64.trunc_f64_s)\n", ind(indent)));
        }
        IRNode::F64_CONVERT_I64_S { value } => {
            emit_ir(value, out, indent);
            out.push_str(&format!("{}(f64.convert_i64_s)\n", ind(indent)));
        }
        IRNode::F64_SQRT { value } => {
            emit_ir(value, out, indent);
            out.push_str(&format!("{}(f64.sqrt)\n", ind(indent)));
        }
        IRNode::BLOCK { label, body } => {
            out.push_str(&format!("{}(block ${}\n", ind(indent), label));
            for n in body {
                emit_ir(n, out, indent + 1);
            }
            out.push_str(&format!("{})\n", ind(indent)));
        }
        IRNode::LOOP { label, body } => {
            out.push_str(&format!("{}(loop ${}\n", ind(indent), label));
            for n in body {
                emit_ir(n, out, indent + 1);
            }
            out.push_str(&format!("{})\n", ind(indent)));
        }
        IRNode::BR { label } => out.push_str(&format!("{}(br ${})\n", ind(indent), label)),
        IRNode::BR_IF { label, cond } => {
            emit_ir(cond, out, indent);
            out.push_str(&format!("{}(br_if ${})\n", ind(indent), label));
        }
        IRNode::IF { cond, then, else_ } => {
            emit_ir(cond, out, indent);
            out.push_str(&format!("{}(if \n", ind(indent)));
            out.push_str(&format!("{}(then\n", ind(indent + 1)));
            for n in then {
                emit_ir(n, out, indent + 2);
            }
            out.push_str(&format!("{})\n", ind(indent + 1)));
            if !else_.is_empty() {
                out.push_str(&format!("{}(else\n", ind(indent + 1)));
                for n in else_ {
                    emit_ir(n, out, indent + 2);
                }
                out.push_str(&format!("{})\n", ind(indent + 1)));
            }
            out.push_str(&format!("{})\n", ind(indent)));
        }
        IRNode::RETURN { value } => {
            emit_ir(value, out, indent);
            out.push_str(&format!("{}(return)\n", ind(indent)));
        }
    }
}

pub fn ir_to_wat(name: &str, export: &str, ir: &IRFunction) -> String {
    let mut wat = String::new();
    wat.push_str("(module\n");
    wat.push_str(&format!("  (func ${name} "));
    for p in &ir.params {
        wat.push_str(&format!("(param ${} {} )", p.name, ir_type_to_wat(&p.ty)));
    }
    if ir.body.iter().any(|n| {
        matches!(
            n,
            IRNode::F64_CONST { .. } | IRNode::F64_SQRT { .. } | IRNode::F64_CONVERT_I64_S { .. }
        )
    }) {
        wat.push_str("(result f64 )");
    }
    wat.push('\n');
    for l in &ir.locals {
        wat.push_str(&format!(
            "    (local ${} {})\n",
            l.name,
            ir_type_to_wat(&l.ty)
        ));
    }
    for node in &ir.body {
        emit_ir(node, &mut wat, 2);
    }
    wat.push_str("  )\n");
    wat.push_str(&format!("  (export \"{export}\" (func ${name}))\n"));
    wat.push(')');
    wat
}
