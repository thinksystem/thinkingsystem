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

use super::CastToken;
use crate::database::SurrealToken;
use std::collections::HashSet;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Parameter {
    pub name: String,
    pub param_type: Option<CastToken>,
}
impl Parameter {
    pub fn new(name: String, param_type: Option<CastToken>) -> Self {
        Self { name, param_type }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClosureToken {
    pub parameters: Vec<Parameter>,
    pub return_type: Option<CastToken>,
    pub body: Box<SurrealToken>,
}
impl ClosureToken {
    pub fn new(
        parameters: Vec<Parameter>,
        return_type: Option<CastToken>,
        body: Box<SurrealToken>,
    ) -> Self {
        Self {
            parameters,
            return_type,
            body,
        }
    }
    pub fn with_return_type(mut self, return_type: CastToken) -> Self {
        self.return_type = Some(return_type);
        self
    }
    pub fn add_parameter(&mut self, param: Parameter) -> Result<(), String> {
        if self.parameters.iter().any(|p| p.name == param.name) {
            return Err(format!("Duplicate parameter name: {}", param.name));
        }
        self.parameters.push(param);
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        let mut seen = HashSet::new();
        for param in &self.parameters {
            if !param.name.starts_with('$') {
                return Err(format!(
                    "Parameter name must start with '$': {}",
                    param.name
                ));
            }
            if !seen.insert(&param.name) {
                return Err(format!("Duplicate parameter name: {}", param.name));
            }
        }
        Ok(())
    }
}
impl std::fmt::Display for ClosureToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params = self
            .parameters
            .iter()
            .map(|p| {
                if let Some(ptype) = &p.param_type {
                    format!("{}: {}", p.name, ptype)
                } else {
                    p.name.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let return_str = self
            .return_type
            .as_ref()
            .map_or("".to_string(), |rt| format!(" -> {rt}"));
        write!(f, "|{}|{} {{ {} }}", params, return_str, self.body)
    }
}
