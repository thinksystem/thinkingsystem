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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct IdiomToken {
    pub path: Vec<String>,
    pub parts: Vec<IdiomPart>,
    pub recursive_depth: Option<u8>,
    pub graph_context: Option<GraphContext>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IdiomPart {
    Field(String),
    Index(usize),
    Method(String),
    Graph(String),
    Optional,
    Recursive(u8),
    CurrentRecord,
    All,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GraphContext {
    pub direction: GraphDirection,
    pub filters: Vec<String>,
    pub table_types: Vec<String>,
    pub depth_range: Option<(u8, u8)>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GraphDirection {
    Outgoing,
    Incoming,
    Bidirectional,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestructureContext {
    pub fields: Vec<String>,
    pub aliases: HashMap<String, String>,
    pub nested_depth: usize,
    pub omitted_fields: Vec<String>,
}
impl IdiomToken {
    pub fn new(path: Vec<String>, parts: Vec<IdiomPart>) -> Self {
        Self {
            path,
            parts,
            recursive_depth: None,
            graph_context: None,
        }
    }
    pub fn from_path(path: &str) -> Self {
        let parts = if path.is_empty() {
            vec![]
        } else {
            vec![IdiomPart::Field(path.to_string())]
        };
        Self {
            path: vec![path.to_string()],
            parts,
            recursive_depth: None,
            graph_context: None,
        }
    }
    pub fn from_segments(segments: Vec<&str>) -> Self {
        let path = segments.iter().map(|s| s.to_string()).collect();
        let parts = segments
            .iter()
            .map(|s| IdiomPart::Field(s.to_string()))
            .collect();
        Self {
            path,
            parts,
            recursive_depth: None,
            graph_context: None,
        }
    }
    pub fn with_recursive_depth(mut self, depth: u8) -> Result<Self, String> {
        match depth {
            1..=255 => {
                self.recursive_depth = Some(depth);
                Ok(self)
            }
            _ => Err("Recursive depth must be between 1 and 255".to_string()),
        }
    }
    pub fn with_graph_context(mut self, context: GraphContext) -> Self {
        self.graph_context = Some(context);
        self
    }
    pub fn field(mut self, field: &str) -> Self {
        self.parts.push(IdiomPart::Field(field.to_string()));
        self
    }
    pub fn index(mut self, idx: usize) -> Self {
        self.parts.push(IdiomPart::Index(idx));
        self
    }
    pub fn method(mut self, method: &str) -> Self {
        self.parts.push(IdiomPart::Method(method.to_string()));
        self
    }
    pub fn graph(mut self, relation: &str) -> Self {
        self.parts.push(IdiomPart::Graph(relation.to_string()));
        self
    }
    pub fn optional(mut self) -> Self {
        self.parts.push(IdiomPart::Optional);
        self
    }
    pub fn current_record(mut self) -> Self {
        self.parts.push(IdiomPart::CurrentRecord);
        self
    }
    pub fn recursive(mut self, depth: u8) -> Self {
        self.parts.push(IdiomPart::Recursive(depth));
        self
    }
    pub fn all(mut self) -> Self {
        self.parts.push(IdiomPart::All);
        self
    }
    pub fn validate(&self) -> Result<(), String> {
        if let Some(ref graph_ctx) = self.graph_context {
            graph_ctx.validate()?;
        }
        if let Some(depth) = self.recursive_depth {
            if depth == 0 {
                return Err("Recursive depth must be at least 1".to_string());
            }
        }
        for part in &self.parts {
            match part {
                IdiomPart::Recursive(depth) if *depth == 0 => {
                    return Err("Recursive part depth must be at least 1".to_string());
                }
                IdiomPart::Field(field) if field.is_empty() => {
                    return Err("Field name cannot be empty".to_string());
                }
                IdiomPart::Method(method) if method.is_empty() => {
                    return Err("Method name cannot be empty".to_string());
                }
                IdiomPart::Graph(relation) if relation.is_empty() => {
                    return Err("Graph relation cannot be empty".to_string());
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub fn validate_destructure(&self, context: &DestructureContext) -> Result<(), String> {
        context.validate()
    }
    pub fn to_select_query(&self) -> crate::database::query_builder::SelectQuery {
        crate::database::surreal_token::SurrealTokenParser::convert_idiom_to_select_query(self)
    }
    pub fn is_complex_query(&self) -> bool {
        self.parts.len() > 2 || self.recursive_depth.is_some()
    }
    pub fn has_graph_traversal(&self) -> bool {
        self.parts
            .iter()
            .any(|part| matches!(part, IdiomPart::Graph(_)))
            || self.graph_context.is_some()
    }
    pub fn has_recursive_operations(&self) -> bool {
        self.recursive_depth.is_some()
            || self
                .parts
                .iter()
                .any(|part| matches!(part, IdiomPart::Recursive(_)))
    }
    pub fn get_referenced_fields(&self) -> Vec<String> {
        let mut fields = Vec::new();
        fields.extend(self.path.clone());
        for part in &self.parts {
            match part {
                IdiomPart::Field(field) => fields.push(field.clone()),
                IdiomPart::Graph(relation) => fields.push(relation.clone()),
                IdiomPart::Method(method) => fields.push(method.clone()),
                _ => {}
            }
        }
        if let Some(ref ctx) = self.graph_context {
            fields.extend(ctx.table_types.clone());
        }
        fields
    }
    pub fn get_referenced_tables(&self) -> Vec<String> {
        let mut tables = Vec::new();
        if !self.path.is_empty() {
            tables.push(self.path[0].clone());
        }
        for part in &self.parts {
            if let IdiomPart::Graph(relation) = part {
                tables.push(relation.clone());
            }
        }
        if let Some(ref ctx) = self.graph_context {
            tables.extend(ctx.table_types.clone());
        }
        tables
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn depth(&self) -> usize {
        self.parts.len()
    }
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty() && self.path.is_empty()
    }
    pub fn path_string(&self) -> String {
        self.path.join(".")
    }
}
impl GraphContext {
    pub fn new(direction: GraphDirection) -> Self {
        Self {
            direction,
            filters: Vec::new(),
            table_types: Vec::new(),
            depth_range: None,
        }
    }
    pub fn outgoing() -> Self {
        Self::new(GraphDirection::Outgoing)
    }
    pub fn incoming() -> Self {
        Self::new(GraphDirection::Incoming)
    }
    pub fn bidirectional() -> Self {
        Self::new(GraphDirection::Bidirectional)
    }
    pub fn with_depth_range(mut self, min: u8, max: u8) -> Result<Self, String> {
        if min > max {
            return Err(
                "Invalid depth range. Minimum depth cannot be greater than maximum depth."
                    .to_string(),
            );
        }
        self.depth_range = Some((min, max));
        Ok(self)
    }
    pub fn with_depth(self, depth: u8) -> Result<Self, String> {
        self.with_depth_range(depth, depth)
    }
    pub fn add_filter(&mut self, filter: String) {
        self.filters.push(filter);
    }
    pub fn filter(mut self, filter: &str) -> Self {
        self.filters.push(filter.to_string());
        self
    }
    pub fn add_table_type(&mut self, table_type: String) {
        self.table_types.push(table_type);
    }
    pub fn table_type(mut self, table_type: &str) -> Self {
        self.table_types.push(table_type.to_string());
        self
    }
    pub fn validate(&self) -> Result<(), String> {
        if let Some((min, _)) = self.depth_range {
            if min == 0 {
                return Err("Found 0 for bound but expected at least 1.".to_string());
            }
        }
        if self.table_types.is_empty() && !self.filters.is_empty() {
            return Err("Table types required when filters are specified".to_string());
        }
        Ok(())
    }
    pub fn is_bidirectional(&self) -> bool {
        matches!(self.direction, GraphDirection::Bidirectional)
    }
    pub fn is_outgoing(&self) -> bool {
        matches!(self.direction, GraphDirection::Outgoing)
    }
    pub fn is_incoming(&self) -> bool {
        matches!(self.direction, GraphDirection::Incoming)
    }
    pub fn get_traversal_operator(&self) -> &'static str {
        match self.direction {
            GraphDirection::Outgoing => "->",
            GraphDirection::Incoming => "<-",
            GraphDirection::Bidirectional => "<->",
        }
    }
}
impl DestructureContext {
    pub fn new(fields: Vec<String>) -> Self {
        Self {
            fields,
            aliases: HashMap::new(),
            nested_depth: 0,
            omitted_fields: Vec::new(),
        }
    }
    pub fn from_fields(fields: &[&str]) -> Self {
        let fields = fields.iter().map(|s| s.to_string()).collect();
        Self::new(fields)
    }
    pub fn with_nested_depth(mut self, depth: usize) -> Self {
        self.nested_depth = depth;
        self
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.nested_depth > 10 {
            return Err("Destructuring depth exceeds maximum of 10 levels".to_string());
        }
        for field in self.aliases.values() {
            if self.omitted_fields.contains(&field.to_string()) {
                return Err(format!("Cannot alias omitted field '{field}'"));
            }
        }
        Ok(())
    }
    pub fn add_alias(&mut self, field: String, alias: String) -> Result<(), String> {
        if self.omitted_fields.contains(&field) {
            return Err(format!("Cannot alias omitted field '{field}'"));
        }
        self.aliases.insert(alias, field);
        Ok(())
    }
    pub fn alias(mut self, field: &str, alias: &str) -> Result<Self, String> {
        self.add_alias(field.to_string(), alias.to_string())?;
        Ok(self)
    }
    pub fn omit_field(&mut self, field: String) -> Result<(), String> {
        for (alias, aliased_field) in &self.aliases {
            if *aliased_field == field {
                return Err(format!(
                    "Cannot omit field '{field}' that is aliased as '{alias}'"
                ));
            }
        }
        self.omitted_fields.push(field);
        Ok(())
    }
    pub fn omit(mut self, field: &str) -> Result<Self, String> {
        self.omit_field(field.to_string())?;
        Ok(self)
    }
    pub fn get_effective_fields(&self) -> Vec<String> {
        self.fields
            .iter()
            .filter(|field| !self.omitted_fields.contains(field))
            .cloned()
            .collect()
    }
    pub fn is_field_omitted(&self, field: &str) -> bool {
        self.omitted_fields.contains(&field.to_string())
    }
    pub fn has_alias(&self, field: &str) -> bool {
        self.aliases.values().any(|f| f == field)
    }
    pub fn get_alias(&self, field: &str) -> Option<&String> {
        self.aliases
            .iter()
            .find(|(_, f)| *f == field)
            .map(|(alias, _)| alias)
    }
    pub fn get_aliases(&self) -> &HashMap<String, String> {
        &self.aliases
    }
    pub fn get_omitted_fields(&self) -> &Vec<String> {
        &self.omitted_fields
    }
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }
    pub fn effective_field_count(&self) -> usize {
        self.get_effective_fields().len()
    }
}
impl Default for GraphContext {
    fn default() -> Self {
        Self::new(GraphDirection::Outgoing)
    }
}
impl Default for DestructureContext {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}
impl std::fmt::Display for IdiomToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut result = String::new();
        if !self.path.is_empty() {
            result.push_str(&self.path.join("."));
        }
        for part in &self.parts {
            match part {
                IdiomPart::Field(field) => {
                    if !result.is_empty() && !result.ends_with('.') {
                        result.push('.');
                    }
                    result.push_str(field);
                }
                IdiomPart::Index(idx) => {
                    result.push_str(&format!("[{idx}]"));
                }
                IdiomPart::Method(method) => {
                    result.push_str(&format!(".{method}()"));
                }
                IdiomPart::Graph(relation) => {
                    result.push_str(&format!("->{relation}"));
                }
                IdiomPart::Optional => {
                    result.push('?');
                }
                IdiomPart::CurrentRecord => {
                    result.push('@');
                }
                IdiomPart::Recursive(depth) => {
                    result.push_str(&format!("{{{depth}}}"));
                }
                IdiomPart::All => {
                    result.push_str("[*]");
                }
            }
        }
        write!(f, "{result}")
    }
}
impl std::fmt::Display for GraphDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let direction_str = match self {
            GraphDirection::Outgoing => "->",
            GraphDirection::Incoming => "<-",
            GraphDirection::Bidirectional => "<->",
        };
        write!(f, "{direction_str}")
    }
}
impl std::fmt::Display for GraphContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut result = format!("direction: {}", self.direction);
        if let Some((min, max)) = self.depth_range {
            result.push_str(&format!(", depth: {min}..{max}"));
        }
        if !self.table_types.is_empty() {
            result.push_str(&format!(", tables: [{}]", self.table_types.join(", ")));
        }
        if !self.filters.is_empty() {
            result.push_str(&format!(", filters: [{}]", self.filters.join(", ")));
        }
        write!(f, "{{{result}}}")
    }
}
impl std::fmt::Display for DestructureContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut result = format!("fields: [{}]", self.fields.join(", "));
        if !self.aliases.is_empty() {
            let aliases: Vec<String> = self
                .aliases
                .iter()
                .map(|(alias, field)| format!("{alias}: {field}"))
                .collect();
            result.push_str(&format!(", aliases: {{{}}}", aliases.join(", ")));
        }
        if !self.omitted_fields.is_empty() {
            result.push_str(&format!(", omitted: [{}]", self.omitted_fields.join(", ")));
        }
        if self.nested_depth > 0 {
            result.push_str(&format!(", depth: {}", self.nested_depth));
        }
        write!(f, "{{{result}}}")
    }
}
impl From<&str> for IdiomToken {
    fn from(s: &str) -> Self {
        Self::from_path(s)
    }
}
impl From<String> for IdiomToken {
    fn from(s: String) -> Self {
        Self::from_path(&s)
    }
}
impl From<Vec<&str>> for IdiomToken {
    fn from(segments: Vec<&str>) -> Self {
        Self::from_segments(segments)
    }
}
impl From<&[&str]> for IdiomToken {
    fn from(segments: &[&str]) -> Self {
        Self::from_segments(segments.to_vec())
    }
}
impl IntoIterator for IdiomToken {
    type Item = IdiomPart;
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.parts.into_iter()
    }
}
impl<'a> IntoIterator for &'a IdiomToken {
    type Item = &'a IdiomPart;
    type IntoIter = std::slice::Iter<'a, IdiomPart>;
    fn into_iter(self) -> Self::IntoIter {
        self.parts.iter()
    }
}
impl IdiomPart {
    pub fn is_field(&self) -> bool {
        matches!(self, IdiomPart::Field(_))
    }
    pub fn is_index(&self) -> bool {
        matches!(self, IdiomPart::Index(_))
    }
    pub fn is_method(&self) -> bool {
        matches!(self, IdiomPart::Method(_))
    }
    pub fn is_graph(&self) -> bool {
        matches!(self, IdiomPart::Graph(_))
    }
    pub fn is_optional(&self) -> bool {
        matches!(self, IdiomPart::Optional)
    }
    pub fn is_recursive(&self) -> bool {
        matches!(self, IdiomPart::Recursive(_))
    }
    pub fn as_string(&self) -> Option<&String> {
        match self {
            IdiomPart::Field(s) | IdiomPart::Method(s) | IdiomPart::Graph(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_index(&self) -> Option<usize> {
        match self {
            IdiomPart::Index(idx) => Some(*idx),
            _ => None,
        }
    }
    pub fn as_recursive_depth(&self) -> Option<u8> {
        match self {
            IdiomPart::Recursive(depth) => Some(*depth),
            _ => None,
        }
    }
}
impl std::fmt::Display for IdiomPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdiomPart::Field(field) => write!(f, "{field}"),
            IdiomPart::Index(idx) => write!(f, "[{idx}]"),
            IdiomPart::Method(method) => write!(f, "{method}()"),
            IdiomPart::Graph(relation) => write!(f, "->{relation}"),
            IdiomPart::Optional => write!(f, "?"),
            IdiomPart::CurrentRecord => write!(f, "@"),
            IdiomPart::Recursive(depth) => write!(f, "{{{depth}}}"),
            IdiomPart::All => write!(f, "[*]"),
        }
    }
}
