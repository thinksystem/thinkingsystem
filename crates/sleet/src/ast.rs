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

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<AstNode>),
    Object(HashMap<String, AstNode>),
    JsonValue(Value),
}

impl From<Value> for Literal {
    fn from(value: Value) -> Literal {
        match value {
            Value::Null => Literal::Null,
            Value::Bool(b) => Literal::Bool(b),
            Value::Number(n) => Literal::Number(n.as_f64().unwrap_or(0.0)),
            Value::String(s) => Literal::String(s),
            Value::Array(arr) => {
                let nodes = arr
                    .into_iter()
                    .map(|v| AstNode::from(Op::Literal(Literal::from(v))))
                    .collect();
                Literal::Array(nodes)
            }
            Value::Object(obj) => {
                let nodes = obj
                    .into_iter()
                    .map(|(k, v)| (k, AstNode::from(Op::Literal(Literal::from(v)))))
                    .collect();
                Literal::Object(nodes)
            }
        }
    }
}

impl From<Literal> for Value {
    fn from(literal: Literal) -> Value {
        match literal {
            Literal::Null => Value::Null,
            Literal::Bool(b) => Value::Bool(b),
            Literal::Number(n) => Value::Number(
                serde_json::Number::from_f64(n).unwrap_or_else(|| serde_json::Number::from(0)),
            ),
            Literal::String(s) => Value::String(s),
            Literal::JsonValue(v) => v,
            Literal::Array(nodes) => {
                let values: Vec<Value> = nodes
                    .into_iter()
                    .filter_map(|node| {
                        if let Op::Literal(lit) = node.op {
                            Some(lit.into())
                        } else {
                            None
                        }
                    })
                    .collect();
                Value::Array(values)
            }
            Literal::Object(nodes) => {
                let map: serde_json::Map<String, Value> = nodes
                    .into_iter()
                    .filter_map(|(k, node)| {
                        if let Op::Literal(lit) = node.op {
                            Some((k, lit.into()))
                        } else {
                            None
                        }
                    })
                    .collect();
                Value::Object(map)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PathSegment {
    State,
    Input,
    Key(String),
    Index(u64),
    DynamicOffset(Box<AstNode>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Path(pub Vec<PathSegment>);

impl From<Vec<String>> for Path {
    fn from(string_path: Vec<String>) -> Self {
        let segments = string_path.into_iter().map(PathSegment::Key).collect();
        Path(segments)
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path_str = self
            .0
            .iter()
            .map(|segment| match segment {
                PathSegment::State => "state".to_string(),
                PathSegment::Input => "input".to_string(),
                PathSegment::Key(key) => key.clone(),
                PathSegment::Index(idx) => format!("[{idx}]"),
                PathSegment::DynamicOffset(_) => "[dynamic]".to_string(),
            })
            .collect::<Vec<_>>()
            .join(".");
        write!(f, "{path_str}")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Op {
    Literal(Literal),
    Sequence(Vec<AstNode>),
    If {
        condition: Box<AstNode>,
        then_branch: Box<AstNode>,
        else_branch: Option<Box<AstNode>>,
    },

    Fetch(Path),
    Assign {
        path: Path,
        value: Box<AstNode>,
    },

    SetNextBlock(String),
    Terminate,

    Await {
        interaction_id: String,
        agent_id: String,
        prompt: Option<Box<AstNode>>,
        timeout_ms: Option<u64>,
    },

    Evaluate {
        bytecode: Vec<u8>,
        output_path: Path,
    },

    PushErrorHandler {
        catch_block_id: String,
    },
    PopErrorHandler,

    Add(Box<AstNode>, Box<AstNode>),
    Subtract(Box<AstNode>, Box<AstNode>),
    Multiply(Box<AstNode>, Box<AstNode>),
    Divide(Box<AstNode>, Box<AstNode>),
    Modulo(Box<AstNode>, Box<AstNode>),
    Negate(Box<AstNode>),

    Equal(Box<AstNode>, Box<AstNode>),
    NotEqual(Box<AstNode>, Box<AstNode>),
    LessThan(Box<AstNode>, Box<AstNode>),
    GreaterThan(Box<AstNode>, Box<AstNode>),
    LessEqual(Box<AstNode>, Box<AstNode>),
    GreaterEqual(Box<AstNode>, Box<AstNode>),

    And(Box<AstNode>, Box<AstNode>),
    Or(Box<AstNode>, Box<AstNode>),
    Not(Box<AstNode>),

    Length(Box<AstNode>),
    Index {
        object: Box<AstNode>,
        index: Box<AstNode>,
    },

    Call {
        callee: Box<AstNode>,
        args: Vec<AstNode>,
    },

    Conditional {
        condition: Box<AstNode>,
        then_expr: Box<AstNode>,
        else_expr: Box<AstNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AstNode {
    pub op: Op,
    pub metadata: HashMap<String, Value>,
    pub source_location: Option<SourceLocation>,
}

impl From<Op> for AstNode {
    fn from(op: Op) -> Self {
        AstNode {
            op,
            metadata: HashMap::new(),
            source_location: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub version: String,
    pub start_block_id: String,
    pub blocks: HashMap<String, AstNode>,
    pub initial_state: AstNode,
    pub permissions: Value,
    pub participants: Vec<String>,
}

pub mod legacy {
    use super::*;

    pub fn convert_string_path_to_path(string_path: Vec<String>) -> Path {
        Path::from(string_path)
    }

    pub fn convert_path_to_string_path(path: &Path) -> Vec<String> {
        path.0
            .iter()
            .filter_map(|segment| match segment {
                PathSegment::State => None,
                PathSegment::Input => Some("input".to_string()),
                PathSegment::Key(key) => Some(key.clone()),
                PathSegment::Index(idx) => Some(idx.to_string()),
                PathSegment::DynamicOffset(_) => Some("dynamic".to_string()),
            })
            .collect()
    }

    pub fn convert_value_to_literal(value: Value) -> Literal {
        Literal::from(value)
    }

    pub fn convert_literal_to_value(literal: Literal) -> Value {
        literal.into()
    }
}
