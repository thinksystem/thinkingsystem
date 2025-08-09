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

use crate::database::*;
use std::cmp::Ordering;
use std::collections::HashMap;
#[derive(Debug)]
pub struct ObjectToken {
    pub fields: HashMap<String, SurrealToken>,
    pub nested_depth: usize,
}
impl ObjectToken {
    pub fn new() -> Self {
        Self {
            fields: HashMap::new(),
            nested_depth: 0,
        }
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            fields: HashMap::with_capacity(capacity),
            nested_depth: 0,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.nested_depth > 10 {
            return Err("Object nesting depth exceeds maximum of 10".to_string());
        }
        Ok(())
    }
    pub fn insert(&mut self, key: String, value: SurrealToken) -> Result<(), String> {
        self.fields.insert(key, value);
        Ok(())
    }
    pub fn get(&self, key: &str) -> Option<&SurrealToken> {
        self.fields.get(key)
    }
    pub fn remove(&mut self, key: &str) -> Option<SurrealToken> {
        self.fields.remove(key)
    }
    pub fn extend(&mut self, other: &ObjectToken) {
        for (key, value) in other.fields.iter() {
            self.fields.insert(key.clone(), value.clone());
        }
    }
    pub fn remove_many(&mut self, keys: &[&str]) {
        for key in keys {
            self.fields.remove(*key);
        }
    }
    pub fn len(&self) -> usize {
        self.fields.len()
    }
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
    pub fn clear(&mut self) {
        self.fields.clear();
    }
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.fields.keys()
    }
    pub fn values(&self) -> impl Iterator<Item = &SurrealToken> {
        self.fields.values()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&String, &SurrealToken)> {
        self.fields.iter()
    }
}
impl Clone for ObjectToken {
    fn clone(&self) -> Self {
        Self {
            fields: self.fields.clone(),
            nested_depth: self.nested_depth,
        }
    }
}
impl PartialEq for ObjectToken {
    fn eq(&self, other: &Self) -> bool {
        self.fields == other.fields
    }
}
impl PartialOrd for ObjectToken {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self.fields.len().cmp(&other.fields.len()) {
            Ordering::Equal => {
                let mut self_keys: Vec<_> = self.fields.keys().collect();
                let mut other_keys: Vec<_> = other.fields.keys().collect();
                self_keys.sort();
                other_keys.sort();
                for (k1, k2) in self_keys.iter().zip(other_keys.iter()) {
                    match k1.cmp(k2) {
                        Ordering::Equal => {
                            let v1 = self.fields.get(*k1).unwrap();
                            let v2 = other.fields.get(*k2).unwrap();
                            match v1.partial_cmp(v2) {
                                Some(Ordering::Equal) => continue,
                                other => return other,
                            }
                        }
                        other => return Some(other),
                    }
                }
                Some(Ordering::Equal)
            }
            other => Some(other),
        }
    }
}
impl Eq for ObjectToken {}
impl std::fmt::Display for ObjectToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut entries: Vec<_> = self.fields.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        write!(f, "{{")?;
        for (i, (key, value)) in entries.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "\"{key}\": {value}")?;
        }
        write!(f, "}}")
    }
}
impl std::hash::Hash for ObjectToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let mut entries: Vec<_> = self.fields.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        for (k, v) in entries {
            k.hash(state);
            v.hash(state);
        }
    }
}
impl Default for ObjectToken {
    fn default() -> Self {
        Self::new()
    }
}
