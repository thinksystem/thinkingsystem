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
use crate::database::*;
use std::collections::HashSet;
use std::iter::FromIterator;
#[derive(Debug)]
pub struct ArrayToken {
    pub elements: Vec<SurrealToken>,
    pub element_type: Option<Box<CastToken>>,
    pub max_length: Option<usize>,
    pub is_set: bool,
}
impl PartialEq for ArrayToken {
    fn eq(&self, other: &Self) -> bool {
        if self.is_set && other.is_set {
            if self.elements.len() != other.elements.len() {
                return false;
            }
            let self_set: HashSet<_> = self.elements.iter().collect();
            let other_set: HashSet<_> = other.elements.iter().collect();
            return self_set == other_set;
        }
        self.elements == other.elements
    }
}
impl PartialOrd for ArrayToken {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.elements.len().cmp(&other.elements.len()))
    }
}
impl Eq for ArrayToken {}
impl Clone for ArrayToken {
    fn clone(&self) -> Self {
        Self {
            elements: self.elements.clone(),
            element_type: self.element_type.clone(),
            max_length: self.max_length,
            is_set: self.is_set,
        }
    }
}
impl std::fmt::Display for ArrayToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bracket_style = if self.is_set { ("{", "}") } else { ("[", "]") };
        write!(
            f,
            "{}{}{}",
            bracket_style.0,
            self.elements
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            bracket_style.1
        )
    }
}
impl std::hash::Hash for ArrayToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.is_set.hash(state);
        self.element_type.hash(state);
        self.max_length.hash(state);
        if self.is_set {
            let mut elements = self.elements.clone();
            elements.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            elements.hash(state);
        } else {
            self.elements.hash(state);
        }
    }
}
impl Default for ArrayToken {
    fn default() -> Self {
        Self::new()
    }
}
impl<'a> IntoIterator for &'a ArrayToken {
    type Item = &'a SurrealToken;
    type IntoIter = std::slice::Iter<'a, SurrealToken>;
    fn into_iter(self) -> Self::IntoIter {
        self.elements.iter()
    }
}
impl IntoIterator for ArrayToken {
    type Item = SurrealToken;
    type IntoIter = std::vec::IntoIter<SurrealToken>;
    fn into_iter(self) -> Self::IntoIter {
        self.elements.into_iter()
    }
}
impl FromIterator<SurrealToken> for ArrayToken {
    fn from_iter<T: IntoIterator<Item = SurrealToken>>(iter: T) -> Self {
        Self {
            elements: Vec::from_iter(iter),
            element_type: None,
            max_length: None,
            is_set: false,
        }
    }
}
impl ArrayToken {
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
            element_type: None,
            max_length: None,
            is_set: false,
        }
    }
    pub fn with_element_type(mut self, element_type: CastToken) -> Self {
        self.element_type = Some(Box::new(element_type));
        self
    }
    pub fn with_max_length(mut self, max: usize) -> Self {
        self.max_length = Some(max);
        self
    }
    pub fn as_set(mut self) -> Self {
        self.is_set = true;
        self.deduplicate();
        self
    }
    pub fn validate(&self) -> Result<(), String> {
        self.validate_length()?;
        self.validate_element_types()?;
        Ok(())
    }
    fn validate_length(&self) -> Result<(), String> {
        if let Some(max) = self.max_length {
            if self.elements.len() > max {
                return Err(format!(
                    "Array length {} exceeds maximum {}",
                    self.elements.len(),
                    max
                ));
            }
        }
        Ok(())
    }
    fn validate_element_types(&self) -> Result<(), String> {
        if let Some(type_def) = &self.element_type {
            for element in &self.elements {
                if !matches!(element, SurrealToken::Cast(ref cast) if cast == &**type_def) {
                    return Err("Array contains elements of mismatched type".to_string());
                }
            }
        }
        Ok(())
    }
    pub fn push(&mut self, element: SurrealToken) -> Result<(), String> {
        if let Some(max) = self.max_length {
            if self.elements.len() >= max {
                return Err(format!("Array length would exceed maximum {max}"));
            }
        }
        if let Some(type_def) = &self.element_type {
            if !matches!(element, SurrealToken::Cast(ref cast) if cast == &**type_def) {
                return Err("Element type mismatch".to_string());
            }
        }
        self.elements.push(element);
        if self.is_set {
            self.deduplicate();
        }
        Ok(())
    }
    pub fn get(&self, index: usize) -> Option<&SurrealToken> {
        self.elements.get(index)
    }
    pub fn get_last(&self) -> Option<&SurrealToken> {
        self.elements.last()
    }
    pub fn len(&self) -> usize {
        self.elements.len()
    }
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
    pub fn iter(&self) -> std::slice::Iter<'_, SurrealToken> {
        self.elements.iter()
    }
    pub fn slice(mut self, start: Option<usize>, end: Option<usize>, inclusive: bool) -> Self {
        let end_idx = end.unwrap_or(self.elements.len());
        let range_end = if inclusive {
            end_idx.saturating_add(1)
        } else {
            end_idx
        };
        let start_idx = start.unwrap_or(0);
        if start_idx >= self.elements.len() {
            self.elements.clear();
            return self;
        }
        let final_end = std::cmp::min(range_end, self.elements.len());
        if start_idx >= final_end {
            self.elements.clear();
            return self;
        }
        self.elements = self.elements[start_idx..final_end].to_vec();
        self
    }
    pub fn filter<P>(mut self, predicate: P) -> Self
    where
        P: Fn(&SurrealToken) -> bool,
    {
        self.elements.retain(predicate);
        self
    }
    pub fn map<F>(mut self, transform: F) -> Self
    where
        F: Fn(&SurrealToken, usize) -> SurrealToken,
    {
        self.elements = self
            .elements
            .iter()
            .enumerate()
            .map(|(i, v)| transform(v, i))
            .collect();
        self
    }
    fn deduplicate(&mut self) {
        let mut seen = HashSet::new();
        self.elements.retain(|element| seen.insert(element.clone()));
    }
    pub fn union(mut self, other: &ArrayToken) -> Self {
        self.elements.extend_from_slice(&other.elements);
        self.is_set = true;
        self.deduplicate();
        self.max_length = None;
        self
    }
    pub fn intersection(&self, other: &ArrayToken) -> Self {
        let other_set: HashSet<_> = other.elements.iter().collect();
        let new_elements = self
            .elements
            .iter()
            .filter(|e| other_set.contains(e))
            .cloned()
            .collect();
        ArrayToken {
            elements: new_elements,
            element_type: self.element_type.clone(),
            max_length: self.max_length,
            is_set: true,
        }
    }
    pub fn difference(&self, other: &ArrayToken) -> Self {
        let other_set: HashSet<_> = other.elements.iter().collect();
        let new_elements = self
            .elements
            .iter()
            .filter(|e| !other_set.contains(e))
            .cloned()
            .collect();
        ArrayToken {
            elements: new_elements,
            element_type: self.element_type.clone(),
            max_length: self.max_length,
            is_set: true,
        }
    }
    pub fn symmetric_difference(&self, other: &ArrayToken) -> Self {
        let self_set: HashSet<_> = self.elements.iter().collect();
        let other_set: HashSet<_> = other.elements.iter().collect();
        let mut new_elements = Vec::new();
        for element in &self.elements {
            if !other_set.contains(element) {
                new_elements.push(element.clone());
            }
        }
        for element in &other.elements {
            if !self_set.contains(element) {
                new_elements.push(element.clone());
            }
        }
        ArrayToken {
            elements: new_elements,
            element_type: self.element_type.clone(),
            max_length: None,
            is_set: true,
        }
    }
    pub fn is_subset(&self, other: &ArrayToken) -> bool {
        let other_set: HashSet<_> = other.elements.iter().collect();
        self.elements.iter().all(|e| other_set.contains(e))
    }
    pub fn is_superset(&self, other: &ArrayToken) -> bool {
        other.is_subset(self)
    }
    pub fn is_disjoint(&self, other: &ArrayToken) -> bool {
        let other_set: HashSet<_> = other.elements.iter().collect();
        !self.elements.iter().any(|e| other_set.contains(e))
    }
}
