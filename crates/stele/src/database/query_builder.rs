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

use std::collections::HashMap;
use std::fmt;
use crate::database::sanitize::{sanitize_field_expr, sanitize_record_id, sanitize_table_name};
use tracing::warn;


#[derive(Debug, Clone)]
pub enum DatabaseFunction {
    Array,
    Math,
    String,
    Time,
    Type,
    Value,
    Vector,
}
#[derive(Debug, Clone)]
pub enum DatabaseMethod {
    Flatten,
    Distinct,
    Len,
    First,
    Last,
    Sort,
    Reverse,
    Uppercase,
    Lowercase,
    Trim,
    Split(String),
    Abs,
    Round,
    Floor,
    Ceil,
}
#[derive(Debug, Clone)]
pub enum FunctionCall {
    Array(ArrayFunction),
    Math(MathFunction),
    String(StringFunction),
    Time(TimeFunction),
    Type(TypeFunction),
    Value(ValueFunction),
    Vector(VectorFunction),
}
#[derive(Debug, Clone)]
pub enum ArrayFunction {
    Len(String),
    First(String),
    Last(String),
    Sort(String),
    Reverse(String),
    Flatten(String),
    Distinct(String),
    Join(String, String),
}
#[derive(Debug, Clone)]
pub enum MathFunction {
    Abs(String),
    Round(String, Option<u32>),
    Floor(String),
    Ceil(String),
    Max(String),
    Min(String),
    Sum(String),
}
#[derive(Debug, Clone)]
pub enum StringFunction {
    Len(String),
    Uppercase(String),
    Lowercase(String),
    Trim(String),
    Split(String, String),
}
#[derive(Debug, Clone)]
pub enum TimeFunction {
    Now,
    Format(String, String),
}
#[derive(Debug, Clone)]
pub enum TypeFunction {
    IsArray(String),
    IsString(String),
    IsNumber(String),
    IsBool(String),
    IsNull(String),
}
#[derive(Debug, Clone)]
pub enum ValueFunction {
    Default(String, serde_json::Value),
}
#[derive(Debug, Clone)]
pub enum VectorFunction {
    Similarity(String, Vec<f64>, Option<String>),
}
#[derive(Debug, Clone)]
pub enum PathDirection {
    Outbound,
    Inbound,
    Bidirectional,
}
#[derive(Debug, Clone)]
pub struct GraphPathSegment {
    pub direction: PathDirection,
    pub edge_table: String,
    pub target_node_table: Option<String>,
    pub conditions: Option<Condition>,
}
impl GraphPathSegment {
    pub fn with_condition(mut self, condition: Condition) -> Self {
        self.conditions = Some(condition);
        self
    }
}
#[derive(Debug, Clone)]
pub struct GraphTraversal {
    pub segments: Vec<GraphPathSegment>,
}
#[derive(Debug, Clone)]
pub enum Condition {
    Simple {
        field: String,
        operator: Operator,
        value: serde_json::Value,
    },
    Function {
        function: FunctionCall,
        operator: Operator,
        value: serde_json::Value,
    },
    And(Box<Condition>, Box<Condition>),
    Or(Box<Condition>, Box<Condition>),
    Not(Box<Condition>),
    Group(Box<Condition>),
    Raw(String),
    GraphTraversal(GraphTraversal),
}
impl Condition {
    pub fn simple(field: &str, operator: Operator, value: serde_json::Value) -> Self {
        Self::Simple {
            field: field.to_string(),
            operator,
            value,
        }
    }
    pub fn function(function: FunctionCall, operator: Operator, value: serde_json::Value) -> Self {
        Self::Function {
            function,
            operator,
            value,
        }
    }
    pub fn and(self, other: Condition) -> Self {
        Self::And(Box::new(self), Box::new(other))
    }
    pub fn or(self, other: Condition) -> Self {
        Self::Or(Box::new(self), Box::new(other))
    }
    pub fn negate(self) -> Self {
        Self::Not(Box::new(self))
    }
    pub fn group(self) -> Self {
        Self::Group(Box::new(self))
    }
    pub fn validated_raw(condition: &str) -> Result<Self, String> {
        if condition.trim().is_empty() {
            return Err("Empty condition".to_string());
        }
        let dangerous_patterns = ["DROP", "DELETE", "INSERT", "UPDATE", "CREATE", "ALTER"];
        for pattern in &dangerous_patterns {
            if condition.to_uppercase().contains(pattern) {
                return Err(format!("Potentially dangerous pattern detected: {pattern}"));
            }
        }
        
        let allowed = |c: char| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    ' ' | '_'
                        | '.'
                        | '$'
                        | '@'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '('
                        | ')'
                        | '*'
                        | '\''
                        | '"'
                        | ':'
                        | '-'
                        | '>'
                        | '<'
                        | '|'
                        | '='
                        | '!'
                        | '~'
                        | '+'
                        | '/'
                        | ','
                )
        };
        if !condition.chars().all(allowed) {
            return Err("Condition contains unsupported characters".to_string());
        }
        if condition.len() > 2000 {
            warn!("Raw condition unusually long: {} chars", condition.len());
        }
        Ok(Self::Raw(condition.to_string()))
    }
    pub fn traversal(segments: Vec<GraphPathSegment>) -> Self {
        Self::GraphTraversal(GraphTraversal { segments })
    }
}
impl fmt::Display for FunctionCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FunctionCall::Array(func) => write!(f, "{func}"),
            FunctionCall::Math(func) => write!(f, "{func}"),
            FunctionCall::String(func) => write!(f, "{func}"),
            FunctionCall::Time(func) => write!(f, "{func}"),
            FunctionCall::Type(func) => write!(f, "{func}"),
            FunctionCall::Value(func) => write!(f, "{func}"),
            FunctionCall::Vector(func) => write!(f, "{func}"),
        }
    }
}
impl fmt::Display for ArrayFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArrayFunction::Len(arr) => write!(f, "array::len({})", sanitize_field_expr(arr)),
            ArrayFunction::First(arr) => write!(f, "array::first({})", sanitize_field_expr(arr)),
            ArrayFunction::Last(arr) => write!(f, "array::last({})", sanitize_field_expr(arr)),
            ArrayFunction::Sort(arr) => write!(f, "array::sort({})", sanitize_field_expr(arr)),
            ArrayFunction::Reverse(arr) => {
                write!(f, "array::reverse({})", sanitize_field_expr(arr))
            }
            ArrayFunction::Flatten(arr) => {
                write!(f, "array::flatten({})", sanitize_field_expr(arr))
            }
            ArrayFunction::Distinct(arr) => {
                write!(f, "array::distinct({})", sanitize_field_expr(arr))
            }
            ArrayFunction::Join(arr, sep) => write!(
                f,
                "array::join({}, \"{}\")",
                sanitize_field_expr(arr),
                sep.replace('\"', "\\\"")
            ),
        }
    }
}
impl fmt::Display for MathFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MathFunction::Abs(val) => write!(f, "math::abs({})", sanitize_field_expr(val)),
            MathFunction::Round(val, places) => match places {
                Some(p) => write!(f, "math::round({}, {p})", sanitize_field_expr(val)),
                None => write!(f, "math::round({})", sanitize_field_expr(val)),
            },
            MathFunction::Floor(val) => write!(f, "math::floor({})", sanitize_field_expr(val)),
            MathFunction::Ceil(val) => write!(f, "math::ceil({})", sanitize_field_expr(val)),
            MathFunction::Max(val) => write!(f, "math::max({})", sanitize_field_expr(val)),
            MathFunction::Min(val) => write!(f, "math::min({})", sanitize_field_expr(val)),
            MathFunction::Sum(val) => write!(f, "math::sum({})", sanitize_field_expr(val)),
        }
    }
}
impl fmt::Display for StringFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StringFunction::Len(s) => write!(f, "string::len({})", sanitize_field_expr(s)),
            StringFunction::Uppercase(s) => {
                write!(f, "string::uppercase({})", sanitize_field_expr(s))
            }
            StringFunction::Lowercase(s) => {
                write!(f, "string::lowercase({})", sanitize_field_expr(s))
            }
            StringFunction::Trim(s) => write!(f, "string::trim({})", sanitize_field_expr(s)),
            StringFunction::Split(s, delim) => write!(
                f,
                "string::split({}, \"{}\")",
                sanitize_field_expr(s),
                delim.replace('\"', "\\\"")
            ),
        }
    }
}
impl fmt::Display for TimeFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeFunction::Now => write!(f, "time::now()"),
            TimeFunction::Format(ts, fmt_str) => write!(f, "time::format({ts}, \"{fmt_str}\")"),
        }
    }
}
impl fmt::Display for TypeFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeFunction::IsArray(val) => {
                write!(f, "type::is::array({})", sanitize_field_expr(val))
            }
            TypeFunction::IsString(val) => {
                write!(f, "type::is::string({})", sanitize_field_expr(val))
            }
            TypeFunction::IsNumber(val) => {
                write!(f, "type::is::number({})", sanitize_field_expr(val))
            }
            TypeFunction::IsBool(val) => write!(f, "type::is::bool({})", sanitize_field_expr(val)),
            TypeFunction::IsNull(val) => write!(f, "type::is::null({})", sanitize_field_expr(val)),
        }
    }
}
impl fmt::Display for ValueFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValueFunction::Default(field, default) => {
                write!(f, "value::default({field}, {default})")
            }
        }
    }
}
impl fmt::Display for VectorFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VectorFunction::Similarity(field, query_vec, metric) => {
                let vec_str = query_vec
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                match metric {
                    Some(m) => write!(
                        f,
                        "vector::similarity({}, [{vec_str}], \"{}\")",
                        sanitize_field_expr(field),
                        m.replace('\"', "\\\"")
                    ),
                    None => write!(
                        f,
                        "vector::similarity({}, [{vec_str}])",
                        sanitize_field_expr(field)
                    ),
                }
            }
        }
    }
}
impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Condition::Simple {
                field,
                operator,
                value,
            } => {
                let op_str = operator.to_string();
                let formatted_value = format_value_for_surreal(value);
                write!(
                    f,
                    "{} {op_str} {formatted_value}",
                    sanitize_field_expr(field)
                )
            }
            Condition::Function {
                function,
                operator,
                value,
            } => {
                let op_str = operator.to_string();
                let formatted_value = format_value_for_surreal(value);
                write!(f, "{function} {op_str} {formatted_value}")
            }
            Condition::And(left, right) => write!(f, "{left} AND {right}"),
            Condition::Or(left, right) => write!(f, "{left} OR {right}"),
            Condition::Not(condition) => write!(f, "NOT {condition}"),
            Condition::Group(condition) => write!(f, "({condition})"),
            Condition::Raw(raw) => write!(f, "{raw}"),
            Condition::GraphTraversal(traversal) => write!(f, "{traversal}"),
        }
    }
}
fn format_value_for_surreal(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "\\'")),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::Array(arr) => {
            let elements: Vec<String> = arr.iter().map(format_value_for_surreal).collect();
            format!("[{}]", elements.join(", "))
        }
        serde_json::Value::Object(_) => {
            format!(
                "'{}'",
                serde_json::to_string(value).unwrap().replace('\'', "\\'")
            )
        }
    }
}
#[derive(Debug, Clone)]
pub struct RelateQuery {
    pub from_record: String,
    pub edge_table: String,
    pub to_record: String,
    pub content: Option<serde_json::Value>,
    pub set_fields: HashMap<String, serde_json::Value>,
    pub return_type: Option<ReturnType>,
    pub timeout: Option<String>,
    pub parallel: bool,
    pub only: bool,
}
#[derive(Debug, Clone)]
pub enum ReturnType {
    None,
    Before,
    After,
    Diff,
    Fields(Vec<String>),
}
impl RelateQuery {
    pub fn new(from: String, edge: String, to: String) -> Self {
        Self {
            from_record: from,
            edge_table: edge,
            to_record: to,
            content: None,
            set_fields: HashMap::new(),
            return_type: None,
            timeout: None,
            parallel: false,
            only: false,
        }
    }
    pub fn content(mut self, value: serde_json::Value) -> Self {
        self.content = Some(value);
        self
    }
    pub fn set(mut self, field: &str, value: serde_json::Value) -> Self {
        self.set_fields.insert(field.to_string(), value);
        self
    }
    pub fn return_type(mut self, return_type: ReturnType) -> Self {
        self.return_type = Some(return_type);
        self
    }
    pub fn timeout(mut self, duration: &str) -> Self {
        self.timeout = Some(duration.to_string());
        self
    }
    pub fn parallel(mut self) -> Self {
        self.parallel = true;
        self
    }
    pub fn only(mut self) -> Self {
        self.only = true;
        self
    }
}
#[derive(Debug, Clone)]
pub enum Operator {
    And,
    Or,
    Not,
    Truthy,
    NullOr,
    TruthyOr,
    Equals,
    NotEquals,
    ExactEquals,
    AnyEquals,
    AllEquals,
    FuzzyMatch,
    NotFuzzyMatch,
    AnyFuzzyMatch,
    AllFuzzyMatch,
    LessThan,
    LessThanEquals,
    GreaterThan,
    GreaterThanEquals,
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
    In,
    NotIn,
    Contains,
    ContainsNot,
    ContainsAll,
    ContainsAny,
    ContainsNone,
    Inside,
    NotInside,
    AllInside,
    AnyInside,
    NoneInside,
    Outside,
    Intersects,
    FullTextMatch,
    FullTextMatches,
    Knn(u32, Option<String>),
    KnnHnsw(u32, u32),
}
impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op_str = match self {
            Operator::And => "AND",
            Operator::Or => "OR",
            Operator::Not => "!",
            Operator::Truthy => "!!",
            Operator::NullOr => "??",
            Operator::TruthyOr => "?:",
            Operator::Equals => "=",
            Operator::NotEquals => "!=",
            Operator::ExactEquals => "==",
            Operator::AnyEquals => "?=",
            Operator::AllEquals => "*=",
            Operator::FuzzyMatch => "~",
            Operator::NotFuzzyMatch => "!~",
            Operator::AnyFuzzyMatch => "?~",
            Operator::AllFuzzyMatch => "*~",
            Operator::LessThan => "<",
            Operator::LessThanEquals => "<=",
            Operator::GreaterThan => ">",
            Operator::GreaterThanEquals => ">=",
            Operator::Add => "+",
            Operator::Subtract => "-",
            Operator::Multiply => "*",
            Operator::Divide => "/",
            Operator::Power => "**",
            Operator::In => "IN",
            Operator::NotIn => "NOT IN",
            Operator::Contains => "CONTAINS",
            Operator::ContainsNot => "CONTAINSNOT",
            Operator::ContainsAll => "CONTAINSALL",
            Operator::ContainsAny => "CONTAINSANY",
            Operator::ContainsNone => "CONTAINSNONE",
            Operator::Inside => "INSIDE",
            Operator::NotInside => "NOTINSIDE",
            Operator::AllInside => "ALLINSIDE",
            Operator::AnyInside => "ANYINSIDE",
            Operator::NoneInside => "NONEINSIDE",
            Operator::Outside => "OUTSIDE",
            Operator::Intersects => "INTERSECTS",
            Operator::FullTextMatch => "@@",
            Operator::FullTextMatches => "MATCHES",
            Operator::Knn(k, metric) => {
                return match metric {
                    Some(m) => write!(f, "<|{k},{m}|>"),
                    None => write!(f, "<|{k}|>"),
                };
            }
            Operator::KnnHnsw(k, ef) => {
                return write!(f, "<|{k},{ef}|>");
            }
        };
        write!(f, "{op_str}")
    }
}
#[derive(Debug, Default, Clone)]
pub struct SelectQuery {
    fields: Vec<String>,
    value_only: bool,
    omit_fields: Vec<String>,
    from_targets: Vec<String>,
    only_clause: bool,
    with_indexes: Option<Vec<String>>,
    no_index: bool,
    where_conditions: Option<Condition>,
    split_on: Option<String>,
    group_by: Vec<String>,
    group_all: bool,
    order_by: Vec<OrderClause>,
    limit: Option<u64>,
    start_at: Option<u64>,
    fetch_fields: Vec<String>,
    timeout: Option<String>,
    parallel: bool,
    tempfiles: bool,
    explain: Option<bool>,
    version: Option<String>,
}
#[derive(Debug, Clone)]
pub struct OrderClause {
    field: String,
    direction: OrderDirection,
    modifier: Option<OrderModifier>,
}
#[derive(Debug, Clone)]
pub enum OrderDirection {
    Asc,
    Desc,
}
#[derive(Debug, Clone)]
pub enum OrderModifier {
    Rand,
    Collate,
    Numeric,
}
impl OrderClause {
    pub fn new(field: String, direction: OrderDirection) -> Self {
        Self {
            field,
            direction,
            modifier: None,
        }
    }
    pub fn with_modifier(mut self, modifier: OrderModifier) -> Self {
        self.modifier = Some(modifier);
        self
    }
    pub fn field(&self) -> &str {
        &self.field
    }
    pub fn direction(&self) -> &OrderDirection {
        &self.direction
    }
    pub fn modifier(&self) -> Option<&OrderModifier> {
        self.modifier.as_ref()
    }
}
impl SelectQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn fields(mut self, fields: Vec<String>) -> Self {
        self.fields = fields;
        self
    }
    pub fn value_only(mut self, value_only: bool) -> Self {
        self.value_only = value_only;
        self
    }
    pub fn omit_fields(mut self, fields: Vec<String>) -> Self {
        self.omit_fields = fields;
        self
    }
    pub fn from(mut self, targets: Vec<String>) -> Self {
        self.from_targets = targets;
        self
    }
    pub fn only(mut self, only: bool) -> Self {
        self.only_clause = only;
        self
    }
    pub fn with_indexes(mut self, indexes: Vec<String>) -> Self {
        self.with_indexes = Some(indexes);
        self
    }
    pub fn no_index(mut self) -> Self {
        self.no_index = true;
        self
    }
    pub fn where_condition(mut self, condition: Condition) -> Self {
        match self.where_conditions {
            Some(existing) => {
                self.where_conditions = Some(existing.and(condition));
            }
            None => {
                self.where_conditions = Some(condition);
            }
        }
        self
    }
    pub fn where_traversal(self, segments: Vec<GraphPathSegment>) -> Self {
        let condition = Condition::traversal(segments);
        self.where_condition(condition)
    }
    pub fn where_path_exists(self, segments: Vec<GraphPathSegment>) -> Self {
        let condition = Condition::traversal(segments);
        self.where_condition(condition)
    }
    pub fn where_reachable_via(self, relationship: &str, target_type: &str) -> Self {
        let segment = GraphPathSegment {
            direction: PathDirection::Outbound,
            edge_table: relationship.to_string(),
            target_node_table: Some(target_type.to_string()),
            conditions: None,
        };
        self.where_traversal(vec![segment])
    }
    pub fn where_multi_hop(self, relationships: Vec<&str>, target_type: &str) -> Self {
        let mut segments = Vec::new();
        for (i, rel) in relationships.iter().enumerate() {
            let segment = GraphPathSegment {
                direction: PathDirection::Outbound,
                edge_table: rel.to_string(),
                target_node_table: if i == relationships.len() - 1 {
                    Some(target_type.to_string())
                } else {
                    Some("nodes".to_string())
                },
                conditions: None,
            };
            segments.push(segment);
        }
        self.where_traversal(segments)
    }
    pub fn where_equals(self, field: &str, value: serde_json::Value) -> Self {
        let condition = Condition::simple(field, Operator::Equals, value);
        self.where_condition(condition)
    }
    pub fn where_not_equals(self, field: &str, value: serde_json::Value) -> Self {
        let condition = Condition::simple(field, Operator::NotEquals, value);
        self.where_condition(condition)
    }
    pub fn where_greater_than(self, field: &str, value: serde_json::Value) -> Self {
        let condition = Condition::simple(field, Operator::GreaterThan, value);
        self.where_condition(condition)
    }
    pub fn where_less_than(self, field: &str, value: serde_json::Value) -> Self {
        let condition = Condition::simple(field, Operator::LessThan, value);
        self.where_condition(condition)
    }
    pub fn where_contains(self, field: &str, value: serde_json::Value) -> Self {
        let condition = Condition::simple(field, Operator::Contains, value);
        self.where_condition(condition)
    }
    pub fn where_in(self, field: &str, values: Vec<serde_json::Value>) -> Self {
        let condition = Condition::simple(field, Operator::In, serde_json::Value::Array(values));
        self.where_condition(condition)
    }
    pub fn where_array_len_greater_than(self, field: &str, length: u64) -> Self {
        let function = FunctionCall::Array(ArrayFunction::Len(field.to_string()));
        let condition = Condition::function(
            function,
            Operator::GreaterThan,
            serde_json::Value::Number(serde_json::Number::from(length)),
        );
        self.where_condition(condition)
    }
    pub fn where_string_contains(self, field: &str, substring: &str) -> Self {
        let condition = Condition::simple(
            field,
            Operator::Contains,
            serde_json::Value::String(substring.to_string()),
        );
        self.where_condition(condition)
    }
    #[deprecated(note = "Use where_condition with structured Condition instead")]
    pub fn where_clause(self, condition: String) -> Result<Self, String> {
        let validated_condition = Condition::validated_raw(&condition)?;
        Ok(self.where_condition(validated_condition))
    }
    pub fn split_on(mut self, field: String) -> Self {
        self.split_on = Some(field);
        self
    }
    pub fn group_by(mut self, fields: Vec<String>) -> Self {
        self.group_by = fields;
        self
    }
    pub fn group_all(mut self) -> Self {
        self.group_all = true;
        self
    }
    pub fn order_by(mut self, clauses: Vec<OrderClause>) -> Self {
        self.order_by = clauses;
        self
    }
    pub fn order_by_asc(mut self, field: String) -> Self {
        self.order_by
            .push(OrderClause::new(field, OrderDirection::Asc));
        self
    }
    pub fn order_by_desc(mut self, field: String) -> Self {
        self.order_by
            .push(OrderClause::new(field, OrderDirection::Desc));
        self
    }
    pub fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }
    pub fn start_at(mut self, start: u64) -> Self {
        self.start_at = Some(start);
        self
    }
    pub fn fetch_fields(mut self, fields: Vec<String>) -> Self {
        self.fetch_fields = fields;
        self
    }
    pub fn timeout(mut self, duration: String) -> Self {
        self.timeout = Some(duration);
        self
    }
    pub fn parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }
    pub fn tempfiles(mut self) -> Self {
        self.tempfiles = true;
        self
    }
    pub fn explain(mut self, explain: bool) -> Self {
        self.explain = Some(explain);
        self
    }
    pub fn version(mut self, timestamp: String) -> Self {
        self.version = Some(timestamp);
        self
    }
    pub fn with_array_function(
        self,
        function: ArrayFunction,
        operator: Operator,
        value: serde_json::Value,
    ) -> Self {
        let condition = Condition::function(FunctionCall::Array(function), operator, value);
        self.where_condition(condition)
    }
    pub fn with_math_function(
        self,
        function: MathFunction,
        operator: Operator,
        value: serde_json::Value,
    ) -> Self {
        let condition = Condition::function(FunctionCall::Math(function), operator, value);
        self.where_condition(condition)
    }
    pub fn with_string_function(
        self,
        function: StringFunction,
        operator: Operator,
        value: serde_json::Value,
    ) -> Self {
        let condition = Condition::function(FunctionCall::String(function), operator, value);
        self.where_condition(condition)
    }
    pub fn with_time_function(
        self,
        function: TimeFunction,
        operator: Operator,
        value: serde_json::Value,
    ) -> Self {
        let condition = Condition::function(FunctionCall::Time(function), operator, value);
        self.where_condition(condition)
    }
    pub fn with_type_function(
        self,
        function: TypeFunction,
        operator: Operator,
        value: serde_json::Value,
    ) -> Self {
        let condition = Condition::function(FunctionCall::Type(function), operator, value);
        self.where_condition(condition)
    }
    pub fn with_value_function(
        self,
        function: ValueFunction,
        operator: Operator,
        value: serde_json::Value,
    ) -> Self {
        let condition = Condition::function(FunctionCall::Value(function), operator, value);
        self.where_condition(condition)
    }
    pub fn with_vector_function(
        self,
        function: VectorFunction,
        operator: Operator,
        value: serde_json::Value,
    ) -> Self {
        let condition = Condition::function(FunctionCall::Vector(function), operator, value);
        self.where_condition(condition)
    }
    pub fn knn_search(self, field: &str, vector: Vec<f64>, k: u32, metric: Option<&str>) -> Self {
        let vector_value = serde_json::Value::Array(
            vector
                .into_iter()
                .map(|v| serde_json::Value::Number(serde_json::Number::from_f64(v).unwrap()))
                .collect(),
        );
        let operator = match metric {
            Some(m) => Operator::Knn(k, Some(m.to_string())),
            None => Operator::Knn(k, None),
        };
        let condition = Condition::simple(field, operator, vector_value);
        self.where_condition(condition)
    }
    pub fn hnsw_knn_search(self, field: &str, vector: Vec<f64>, k: u32, ef: u32) -> Self {
        let vector_value = serde_json::Value::Array(
            vector
                .into_iter()
                .map(|v| serde_json::Value::Number(serde_json::Number::from_f64(v).unwrap()))
                .collect(),
        );
        let operator = Operator::KnnHnsw(k, ef);
        let condition = Condition::simple(field, operator, vector_value);
        self.where_condition(condition)
    }
    pub fn fields_ref(&self) -> &Vec<String> {
        &self.fields
    }
    pub fn value_only_ref(&self) -> bool {
        self.value_only
    }
    pub fn omit_fields_ref(&self) -> &Vec<String> {
        &self.omit_fields
    }
    pub fn from_targets_ref(&self) -> &Vec<String> {
        &self.from_targets
    }
    pub fn only_clause_ref(&self) -> bool {
        self.only_clause
    }
    pub fn with_indexes_ref(&self) -> Option<&Vec<String>> {
        self.with_indexes.as_ref()
    }
    pub fn no_index_ref(&self) -> bool {
        self.no_index
    }
    pub fn where_conditions_ref(&self) -> Option<&Condition> {
        self.where_conditions.as_ref()
    }
    pub fn split_on_ref(&self) -> Option<&String> {
        self.split_on.as_ref()
    }
    pub fn group_by_ref(&self) -> &Vec<String> {
        &self.group_by
    }
    pub fn group_all_ref(&self) -> bool {
        self.group_all
    }
    pub fn order_by_ref(&self) -> &Vec<OrderClause> {
        &self.order_by
    }
    pub fn limit_ref(&self) -> Option<u64> {
        self.limit
    }
    pub fn start_at_ref(&self) -> Option<u64> {
        self.start_at
    }
    pub fn fetch_fields_ref(&self) -> &Vec<String> {
        &self.fetch_fields
    }
    pub fn timeout_ref(&self) -> Option<&String> {
        self.timeout.as_ref()
    }
    pub fn parallel_ref(&self) -> bool {
        self.parallel
    }
    pub fn tempfiles_ref(&self) -> bool {
        self.tempfiles
    }
    pub fn explain_ref(&self) -> Option<bool> {
        self.explain
    }
    pub fn version_ref(&self) -> Option<&String> {
        self.version.as_ref()
    }
}
impl fmt::Display for SelectQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut query = String::from("SELECT ");
        if self.value_only {
            query.push_str("VALUE ");
        }
        if self.fields.is_empty() {
            query.push('*');
        } else {
            let fields: Vec<String> = self.fields.iter().map(|s| sanitize_field_expr(s)).collect();
            query.push_str(&fields.join(", "));
        }
        if !self.omit_fields.is_empty() {
            query.push_str(" OMIT ");
            let fields: Vec<String> = self
                .omit_fields
                .iter()
                .map(|s| sanitize_field_expr(s))
                .collect();
            query.push_str(&fields.join(", "));
        }
        query.push_str(" FROM ");
        if self.only_clause {
            query.push_str("ONLY ");
        }
        let froms: Vec<String> = self
            .from_targets
            .iter()
            .map(|s| sanitize_table_name(s))
            .collect();
        query.push_str(&froms.join(", "));
        if let Some(ref indexes) = self.with_indexes {
            query.push_str(" WITH ");
            let idxs: Vec<String> = indexes.iter().map(|s| sanitize_field_expr(s)).collect();
            query.push_str(&idxs.join(", "));
        }
        if self.no_index {
            query.push_str(" NOINDEX");
        }
        if let Some(ref condition) = self.where_conditions {
            query.push_str(" WHERE ");
            query.push_str(&condition.to_string());
        }
        if let Some(ref split) = self.split_on {
            query.push_str(" SPLIT ");
            query.push_str(&sanitize_field_expr(split));
        }
        if !self.group_by.is_empty() {
            query.push_str(" GROUP BY ");
            let groups: Vec<String> = self
                .group_by
                .iter()
                .map(|s| sanitize_field_expr(s))
                .collect();
            query.push_str(&groups.join(", "));
        }
        if self.group_all {
            query.push_str(" GROUP ALL");
        }
        if !self.order_by.is_empty() {
            query.push_str(" ORDER BY ");
            let orders: Vec<String> = self
                .order_by
                .iter()
                .map(|clause| {
                    let mut order_str = sanitize_field_expr(&clause.field);
                    match clause.direction {
                        OrderDirection::Asc => order_str.push_str(" ASC"),
                        OrderDirection::Desc => order_str.push_str(" DESC"),
                    }
                    if let Some(ref modifier) = clause.modifier {
                        match modifier {
                            OrderModifier::Rand => order_str.push_str(" RAND"),
                            OrderModifier::Collate => order_str.push_str(" COLLATE"),
                            OrderModifier::Numeric => order_str.push_str(" NUMERIC"),
                        }
                    }
                    order_str
                })
                .collect();
            query.push_str(&orders.join(", "));
        }
        if let Some(start) = self.start_at {
            query.push_str(&format!(" START AT {start}"));
        }
        if let Some(limit) = self.limit {
            query.push_str(&format!(" LIMIT {limit}"));
        }
        if !self.fetch_fields.is_empty() {
            query.push_str(" FETCH ");
            let fetches: Vec<String> = self
                .fetch_fields
                .iter()
                .map(|s| sanitize_field_expr(s))
                .collect();
            query.push_str(&fetches.join(", "));
        }
        if let Some(ref timeout) = self.timeout {
            query.push_str(&format!(" TIMEOUT {timeout}"));
        }
        if self.parallel {
            query.push_str(" PARALLEL");
        }
        if self.tempfiles {
            query.push_str(" TEMPFILES");
        }
        if let Some(explain) = self.explain {
            if explain {
                query.push_str(" EXPLAIN");
            }
        }
        if let Some(ref version) = self.version {
            query.push_str(&format!(" VERSION {version}"));
        }
        query.push(';');
        write!(f, "{query}")
    }
}
impl fmt::Display for RelateQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut query = String::new();
        query.push_str("RELATE ");
        if self.only {
            query.push_str("ONLY ");
        }
        query.push_str(&format!(
            "{}->{}->{}",
            sanitize_record_id(&self.from_record),
            sanitize_table_name(&self.edge_table),
            sanitize_record_id(&self.to_record)
        ));
        if let Some(content) = &self.content {
            query.push_str(&format!(" CONTENT {content}"));
        }
        if !self.set_fields.is_empty() {
            query.push_str(" SET ");
            let sets: Vec<String> = self
                .set_fields
                .iter()
                .map(|(k, v)| {
                    let formatted_value = match v {
                        serde_json::Value::Number(n) => {
                            if let Some(i) = n.as_i64() {
                                i.to_string()
                            } else if let Some(f) = n.as_f64() {
                                if f.fract() == 0.0 {
                                    format!("{f:.0}")
                                } else {
                                    format!("{f}")
                                        .trim_end_matches('0')
                                        .trim_end_matches('.')
                                        .to_string()
                                }
                            } else {
                                v.to_string()
                            }
                        }
                        serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "\\'")),
                        _ => v.to_string(),
                    };
                    format!("{} = {formatted_value}", sanitize_field_expr(k))
                })
                .collect();
            query.push_str(&sets.join(", "));
        }
        if let Some(return_type) = &self.return_type {
            query.push_str(" RETURN ");
            match return_type {
                ReturnType::None => query.push_str("NONE"),
                ReturnType::Before => query.push_str("BEFORE"),
                ReturnType::After => query.push_str("AFTER"),
                ReturnType::Diff => query.push_str("DIFF"),
                ReturnType::Fields(fields) => {
                    let flds: Vec<String> = fields.iter().map(|s| sanitize_field_expr(s)).collect();
                    query.push_str(&flds.join(", "))
                }
            }
        }
        if let Some(timeout) = &self.timeout {
            query.push_str(&format!(" TIMEOUT {timeout}"));
        }
        if self.parallel {
            query.push_str(" PARALLEL");
        }
        query.push(';');
        write!(f, "{query}")
    }
}
impl fmt::Display for GraphTraversal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut full_path = String::new();
        for segment in &self.segments {
            let direction_str = match segment.direction {
                PathDirection::Outbound => "->",
                PathDirection::Inbound => "<-",
                PathDirection::Bidirectional => "<->",
            };
            full_path.push_str(direction_str);
            full_path.push_str(&sanitize_table_name(&segment.edge_table));
            if let Some(ref target_table) = segment.target_node_table {
                full_path.push_str("->");
                if let Some(ref conditions) = segment.conditions {
                    full_path.push_str(&format!(
                        "({} WHERE {conditions})",
                        sanitize_table_name(target_table)
                    ));
                } else {
                    full_path.push_str(&sanitize_table_name(target_table));
                }
            }
        }
        write!(f, "{full_path}")
    }
}
