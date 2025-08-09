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

use crate::database::query_builder::{Condition, Operator, SelectQuery};
use crate::database::query_validator::{QueryError, QueryNegotiator};
use crate::database::tokens::*;
use serde::Deserialize;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SurrealToken {
    Geometry(GeometryToken),
    Idiom(IdiomToken),
    NullableValue(NullableToken),
    Array(ArrayToken),
    Boolean(BooleanToken),
    DateTime(DateTimeToken),
    Closure(ClosureToken),
    Cast(CastToken),
    Future(FutureToken),
    RecordId(RecordIdToken),
    Literal(LiteralToken),
    Formatter(FormatterToken),
    Number(NumberToken),
    Object(ObjectToken),
    Range(RangeToken),
    String(StringToken),
    UUID(UUIDToken),
}
#[derive(Debug, Deserialize)]
pub struct PromptsConfig {
    pub prompts: HashMap<String, PromptConfig>,
}
#[derive(Debug, Deserialize)]
pub struct PromptConfig {
    pub prompt: String,
}
pub struct SurrealTokenParser {
    llm_prompts: HashMap<String, String>,
    negotiator: Option<QueryNegotiator>,
}
impl SurrealTokenParser {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = "src/database/prompts/surreal_datatypes.yml";
        let config_str = fs::read_to_string(config_path)?;
        let config: PromptsConfig = serde_yml::from_str(&config_str)?;
        let prompts = config
            .prompts
            .into_iter()
            .map(|(k, v)| (k, v.prompt))
            .collect();
        Ok(Self {
            llm_prompts: prompts,
            negotiator: None,
        })
    }
    pub fn with_negotiator(negotiator: QueryNegotiator) -> Self {
        let mut parser = Self::new().unwrap();
        parser.negotiator = Some(negotiator);
        parser
    }
    pub fn parse_with_validation(&mut self, input: &str) -> Result<IdiomToken, QueryError> {
        if let Some(ref negotiator) = self.negotiator {
            negotiator.validate_token_structure(input)?;
        }
        let token = self.parse_idiom(input);
        if let Some(ref negotiator) = self.negotiator {
            negotiator.validate_parsed_token(&token)?;
        }
        Ok(token)
    }

    pub fn get_prompt_for_token_type(&self, token_type: &str) -> Option<&String> {
        self.llm_prompts.get(token_type)
    }

    fn parse_identifier(&mut self, chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
        let mut identifier = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_alphanumeric() || c == '_' {
                identifier.push(chars.next().unwrap());
            } else {
                break;
            }
        }
        identifier
    }
    fn parse_until_char(
        &mut self,
        chars: &mut std::iter::Peekable<std::str::Chars>,
        end_char: char,
    ) -> String {
        let mut content = String::new();
        for c in chars.by_ref() {
            if c == end_char {
                break;
            }
            content.push(c);
        }
        content
    }
    fn parse_destructuring(&mut self, chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
        let mut depth = 1;
        let mut pattern = String::new();
        chars.next();
        for c in chars.by_ref() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => pattern.push(c),
            }
        }
        pattern
    }
    fn parse_graph_traversal(
        &mut self,
        chars: &mut std::iter::Peekable<std::str::Chars>,
    ) -> Option<IdiomPart> {
        if chars.next() == Some('>') {
            let relation = self.parse_identifier(chars);
            Some(IdiomPart::Graph(relation))
        } else {
            None
        }
    }
    fn parse_array_access(
        &mut self,
        chars: &mut std::iter::Peekable<std::str::Chars>,
    ) -> IdiomPart {
        match chars.peek() {
            Some(&'*') => {
                chars.next();
                chars.next();
                IdiomPart::Field("*".to_string())
            }
            Some(&'`') => {
                chars.next();
                chars.next();
                IdiomPart::Field("$".to_string())
            }
            _ => {
                let content = self.parse_until_char(chars, ']');
                if let Ok(index) = content.parse::<usize>() {
                    IdiomPart::Index(index)
                } else {
                    IdiomPart::Field(content)
                }
            }
        }
    }
    fn parse_recursive_depth(
        &mut self,
        chars: &mut std::iter::Peekable<std::str::Chars>,
    ) -> Option<IdiomPart> {
        let depth_str = self.parse_until_char(chars, '}');
        depth_str.parse::<u8>().ok().map(IdiomPart::Recursive)
    }
    pub fn parse_idiom(&mut self, input: &str) -> IdiomToken {
        let mut token = IdiomToken {
            path: Vec::new(),
            parts: Vec::new(),
            recursive_depth: None,
            graph_context: Some(GraphContext {
                direction: GraphDirection::Outgoing,
                depth_range: None,
                filters: Vec::new(),
                table_types: Vec::new(),
            }),
        };
        let mut chars = input.chars().peekable();
        if let Some(&first_char) = chars.peek() {
            if first_char.is_alphabetic() || first_char == '_' {
                let field = self.parse_identifier(&mut chars);
                token.parts.push(IdiomPart::Field(field));
            }
        }
        while let Some(c) = chars.next() {
            match c {
                '@' => token.parts.push(IdiomPart::CurrentRecord),
                '.' => {
                    if let Some(&'{') = chars.peek() {
                        let pattern = self.parse_destructuring(&mut chars);
                        token.parts.push(IdiomPart::Field(pattern));
                    } else {
                        let name = self.parse_identifier(&mut chars);
                        if chars.peek() == Some(&'(') {
                            token.parts.push(IdiomPart::Method(name));
                        } else {
                            token.parts.push(IdiomPart::Field(name));
                        }
                    }
                }
                '[' => token.parts.push(self.parse_array_access(&mut chars)),
                '{' => {
                    if let Some(part) = self.parse_recursive_depth(&mut chars) {
                        token.parts.push(part);
                    }
                }
                '-' => {
                    if let Some(part) = self.parse_graph_traversal(&mut chars) {
                        token.parts.push(part);
                    }
                }
                '?' => token.parts.push(IdiomPart::Optional),
                _ => {
                    if c.is_alphanumeric() || c == '_' {
                        let mut field = c.to_string();
                        field.push_str(&self.parse_identifier(&mut chars));
                        token.parts.push(IdiomPart::Field(field));
                    }
                }
            }
        }
        token
    }
    pub fn convert_idiom_to_select_query(token: &IdiomToken) -> SelectQuery {
        let mut query = SelectQuery::new();
        let mut current_table: Option<String> = None;
        let mut fields = Vec::new();
        let mut has_graph_traversal = false;
        let mut skip_next_field = false;
        if !token.path.is_empty() {
            current_table = Some(token.path[0].clone());
            query = query.from(token.path.clone());
        }
        for (i, part) in token.parts.iter().enumerate() {
            match part {
                IdiomPart::Field(field) => {
                    if skip_next_field {
                        skip_next_field = false;
                        continue;
                    }
                    if i + 1 < token.parts.len() {
                        if let IdiomPart::Graph(_) = &token.parts[i + 1] {
                            skip_next_field = true;
                            continue;
                        }
                    }
                    if field == "*" {
                        query = query.value_only(true);
                    } else if field == "$" {
                        fields.push("$".to_string());
                    } else if field.starts_with("WHERE") {
                        if let Ok(condition) = Condition::validated_raw(field) {
                            query = query.where_condition(condition);
                        }
                    } else {
                        fields.push(field.clone());
                    }
                }
                IdiomPart::Graph(relation) => {
                    has_graph_traversal = true;
                    current_table = Some(relation.clone());
                    query = query.from(vec![relation.clone()]);
                    if let Some(ctx) = &token.graph_context {
                        query = query.only(true);
                        for filter in &ctx.filters {
                            if !filter.is_empty() {
                                if let Ok(condition) = Condition::validated_raw(filter) {
                                    query = query.where_condition(condition);
                                }
                            }
                        }
                        if !ctx.table_types.is_empty() {
                            let type_values: Vec<serde_json::Value> = ctx
                                .table_types
                                .iter()
                                .map(|t| serde_json::Value::String(t.clone()))
                                .collect();
                            let condition = Condition::simple(
                                "type",
                                Operator::In,
                                serde_json::Value::Array(type_values),
                            );
                            query = query.where_condition(condition);
                        }
                        if !ctx.filters.is_empty()
                            || !ctx.table_types.is_empty()
                            || ctx.depth_range.is_some()
                        {
                            let direction_symbol = match ctx.direction {
                                GraphDirection::Outgoing => "->",
                                GraphDirection::Incoming => "<-",
                                GraphDirection::Bidirectional => "<->",
                            };
                            let direction_condition = format!("{direction_symbol}'{relation}'");
                            if let Ok(condition) = Condition::validated_raw(&direction_condition) {
                                query = query.where_condition(condition);
                            }
                        }
                        if let Some((min, max)) = ctx.depth_range {
                            let min_condition = Condition::simple(
                                "depth",
                                Operator::GreaterThanEquals,
                                serde_json::Value::Number(serde_json::Number::from(min)),
                            );
                            let max_condition = Condition::simple(
                                "depth",
                                Operator::LessThanEquals,
                                serde_json::Value::Number(serde_json::Number::from(max)),
                            );
                            query = query.where_condition(min_condition.and(max_condition));
                        }
                    }
                }
                IdiomPart::Index(idx) => {
                    let condition = if let Some(table) = &current_table {
                        Condition::simple(
                            &format!("{table}.id"),
                            Operator::Equals,
                            serde_json::Value::Number(serde_json::Number::from(*idx)),
                        )
                    } else {
                        Condition::simple(
                            "id",
                            Operator::Equals,
                            serde_json::Value::Number(serde_json::Number::from(*idx)),
                        )
                    };
                    query = query.where_condition(condition);
                }
                IdiomPart::Method(method) => match method.as_str() {
                    "count" => {
                        query = query.fields(vec!["count()".to_string()]);
                    }
                    "first" => {
                        query = query.limit(1);
                    }
                    "last" => {
                        query = query.limit(1);
                        query = query.order_by_desc("id".to_string());
                    }
                    _ => {
                        fields.push(format!("{method}()"));
                    }
                },
                IdiomPart::Optional => {
                    if let Some(table) = &current_table {
                        let condition =
                            Condition::simple(table, Operator::NotEquals, serde_json::Value::Null);
                        query = query.where_condition(condition);
                    }
                }
                IdiomPart::CurrentRecord => {
                    let condition =
                        Condition::simple("@", Operator::NotEquals, serde_json::Value::Null);
                    query = query.where_condition(condition);
                }
                IdiomPart::Recursive(depth) => {
                    let condition = Condition::simple(
                        "depth",
                        Operator::LessThanEquals,
                        serde_json::Value::Number(serde_json::Number::from(*depth)),
                    );
                    query = query.where_condition(condition);
                }
                IdiomPart::All => {
                    query = query.value_only(true);
                    fields.push("*".to_string());
                }
            }
        }
        if has_graph_traversal && fields.is_empty() {
        } else if !fields.is_empty() {
            query = query.fields(fields);
        }
        if let Some(depth) = token.recursive_depth {
            let condition = Condition::simple(
                "depth",
                Operator::LessThanEquals,
                serde_json::Value::Number(serde_json::Number::from(depth)),
            );
            query = query.where_condition(condition);
        }
        if token.parts.len() > 2 || token.recursive_depth.is_some() || has_graph_traversal {
            query = query.parallel(true);
        }
        if has_graph_traversal {
            query = query.only(true);
        }
        query
    }
    pub fn to_select_query(&self, token: &IdiomToken) -> SelectQuery {
        Self::convert_idiom_to_select_query(token)
    }
}
impl PartialOrd for SurrealToken {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let self_weight = type_ordering_weight(self);
        let other_weight = type_ordering_weight(other);
        if self_weight != other_weight {
            return Some(self_weight.cmp(&other_weight));
        }
        Some(match (self, other) {
            (SurrealToken::NullableValue(a), SurrealToken::NullableValue(b)) => {
                match (a.is_none, b.is_none, a.is_null, b.is_null) {
                    (true, true, _, _) => Ordering::Equal,
                    (true, false, _, _) => Ordering::Less,
                    (false, true, _, _) => Ordering::Greater,
                    (false, false, true, true) => Ordering::Equal,
                    (false, false, true, false) => Ordering::Less,
                    (false, false, false, true) => Ordering::Greater,
                    (false, false, false, false) => a.field_name.cmp(&b.field_name),
                }
            }
            _ => Ordering::Equal,
        })
    }
}
pub fn type_ordering_weight(token: &SurrealToken) -> u8 {
    match token {
        SurrealToken::NullableValue(n) if n.is_none => 0,
        SurrealToken::NullableValue(n) if n.is_null => 1,
        SurrealToken::NullableValue(_) => 2,
        SurrealToken::Boolean(_) => 2,
        SurrealToken::Number(_) => 3,
        SurrealToken::String(_) => 4,
        SurrealToken::Literal(l)
            if matches!(l.variants.first(), Some(LiteralVariant::Duration(_))) =>
        {
            5
        }
        SurrealToken::DateTime(_) => 6,
        SurrealToken::UUID(_) => 7,
        SurrealToken::Array(_) => 8,
        SurrealToken::Object(_) => 9,
        SurrealToken::Geometry(_) => 10,
        SurrealToken::Literal(l) => {
            if let Some(LiteralVariant::String(ref s)) = l.variants.first() {
                if s.starts_with("<bytes>") {
                    11
                } else {
                    13
                }
            } else {
                13
            }
        }
        SurrealToken::RecordId(_) => 12,
        SurrealToken::Idiom(_)
        | SurrealToken::Closure(_)
        | SurrealToken::Cast(_)
        | SurrealToken::Future(_)
        | SurrealToken::Formatter(_)
        | SurrealToken::Range(_) => 13,
    }
}
impl std::fmt::Display for SurrealToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SurrealToken::String(s) => write!(f, "{}", s.value),
            SurrealToken::Number(n) => write!(f, "{}", n.raw_text),
            _ => write!(f, "{self:?}"),
        }
    }
}
