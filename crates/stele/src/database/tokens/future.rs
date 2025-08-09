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

use crate::database::SurrealToken;
#[derive(Debug)]
pub struct FutureToken {
    pub expression: Box<SurrealToken>,
    pub dependencies: Vec<String>,
}
impl FutureToken {
    pub fn new(expression: SurrealToken) -> Self {
        Self {
            expression: Box::new(expression),
            dependencies: Vec::new(),
        }
    }
    pub fn with_dependencies(mut self, dependencies: Vec<String>) -> Self {
        self.dependencies = dependencies;
        self
    }
    pub fn expression(&self) -> &SurrealToken {
        &self.expression
    }
    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }
    pub fn add_dependency(&mut self, dependency: String) -> &mut Self {
        if !self.dependencies.contains(&dependency) {
            self.dependencies.push(dependency);
        }
        self
    }
    pub fn remove_dependency(&mut self, dependency: &str) -> &mut Self {
        self.dependencies.retain(|d| d != dependency);
        self
    }
    pub fn set_expression(&mut self, expression: SurrealToken) -> &mut Self {
        self.expression = Box::new(expression);
        self
    }
    pub fn clear_dependencies(&mut self) -> &mut Self {
        self.dependencies.clear();
        self
    }
}
impl Clone for FutureToken {
    fn clone(&self) -> Self {
        Self {
            expression: self.expression.clone(),
            dependencies: self.dependencies.clone(),
        }
    }
}
impl PartialEq for FutureToken {
    fn eq(&self, other: &Self) -> bool {
        self.expression == other.expression && self.dependencies == other.dependencies
    }
}
impl Eq for FutureToken {}
impl std::hash::Hash for FutureToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.expression.hash(state);
        self.dependencies.hash(state);
    }
}
impl std::fmt::Display for FutureToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<future> {{ {:?} }}", self.expression)
    }
}
