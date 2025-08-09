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

use crate::blocks::rules::BlockError;
use std::collections::{HashMap, HashSet};
pub struct DependencyManager;
impl DependencyManager {
    pub fn validate_dependencies(
        functions: &HashMap<String, Vec<super::function::DynamicFunction>>,
    ) -> Result<(), BlockError> {
        for (function_name, versions) in functions {
            if let Some(latest_version) = versions.last() {
                for dependency in &latest_version.dependencies {
                    if !functions.contains_key(dependency) {
                        return Err(BlockError::ProcessingError(format!(
                            "Function {function_name} has missing dependency: {dependency}"
                        )));
                    }
                }
            }
        }
        Ok(())
    }
    pub fn get_dependency_graph(
        functions: &HashMap<String, Vec<super::function::DynamicFunction>>,
    ) -> HashMap<String, Vec<String>> {
        functions
            .iter()
            .filter_map(|(name, versions)| {
                versions
                    .last()
                    .map(|latest| (name.clone(), latest.dependencies.clone()))
            })
            .collect()
    }
    pub fn detect_circular_dependencies(
        functions: &HashMap<String, Vec<super::function::DynamicFunction>>,
    ) -> Result<(), BlockError> {
        let graph = Self::get_dependency_graph(functions);
        for function_name in graph.keys() {
            let mut visited = HashSet::new();
            let mut rec_stack = HashSet::new();
            if Self::has_circular_dependency_util(
                function_name,
                &graph,
                &mut visited,
                &mut rec_stack,
            ) {
                return Err(BlockError::ProcessingError(format!(
                    "Circular dependency detected involving function: {function_name}"
                )));
            }
        }
        Ok(())
    }
    fn has_circular_dependency_util(
        function: &str,
        graph: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> bool {
        visited.insert(function.to_string());
        rec_stack.insert(function.to_string());
        if let Some(dependencies) = graph.get(function) {
            for dep in dependencies {
                if !visited.contains(dep) {
                    if Self::has_circular_dependency_util(dep, graph, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.contains(dep) {
                    return true;
                }
            }
        }
        rec_stack.remove(function);
        false
    }
    pub fn get_functions_by_dependency(
        functions: &HashMap<String, Vec<super::function::DynamicFunction>>,
        dependency: &str,
    ) -> Vec<String> {
        functions
            .iter()
            .filter_map(|(name, versions)| {
                if let Some(latest) = versions.last() {
                    if latest.has_dependency(dependency) {
                        Some(name.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn validate_function_chain(
        functions: &HashMap<String, Vec<super::function::DynamicFunction>>,
        chain: &[String],
    ) -> Result<(), BlockError> {
        for func_name in chain {
            if !functions.contains_key(func_name) {
                return Err(BlockError::ProcessingError(format!(
                    "Function {func_name} not found in chain"
                )));
            }
        }
        Ok(())
    }
    pub fn get_dependency_tree(
        functions: &HashMap<String, Vec<super::function::DynamicFunction>>,
        function_name: &str,
    ) -> Option<HashMap<String, Vec<String>>> {
        let graph = Self::get_dependency_graph(functions);
        let mut tree = HashMap::new();
        let mut visited = HashSet::new();
        Self::build_dependency_tree(&graph, function_name, &mut tree, &mut visited);
        if tree.is_empty() {
            None
        } else {
            Some(tree)
        }
    }
    fn build_dependency_tree(
        graph: &HashMap<String, Vec<String>>,
        current: &str,
        tree: &mut HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
    ) {
        if visited.contains(current) {
            return;
        }
        visited.insert(current.to_string());
        if let Some(deps) = graph.get(current) {
            tree.insert(current.to_string(), deps.clone());
            for dep in deps {
                Self::build_dependency_tree(graph, dep, tree, visited);
            }
        }
    }
    pub fn topological_sort(
        functions: &HashMap<String, Vec<super::function::DynamicFunction>>,
    ) -> Result<Vec<String>, BlockError> {
        let graph = Self::get_dependency_graph(functions);
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut result = Vec::new();
        for function_name in graph.keys() {
            if !visited.contains(function_name) {
                Self::topological_sort_util(
                    &graph,
                    function_name,
                    &mut visited,
                    &mut rec_stack,
                    &mut result,
                )?;
            }
        }
        result.reverse();
        Ok(result)
    }
    fn topological_sort_util(
        graph: &HashMap<String, Vec<String>>,
        current: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        result: &mut Vec<String>,
    ) -> Result<(), BlockError> {
        visited.insert(current.to_string());
        rec_stack.insert(current.to_string());
        if let Some(dependencies) = graph.get(current) {
            for dep in dependencies {
                if !visited.contains(dep) {
                    Self::topological_sort_util(graph, dep, visited, rec_stack, result)?;
                } else if rec_stack.contains(dep) {
                    return Err(BlockError::ProcessingError(format!(
                        "Circular dependency detected: {current} -> {dep}"
                    )));
                }
            }
        }
        rec_stack.remove(current);
        result.push(current.to_string());
        Ok(())
    }
}
