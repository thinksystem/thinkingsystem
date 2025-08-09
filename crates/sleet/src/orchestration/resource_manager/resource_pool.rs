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

use super::types::{HasId, ResourceType};
use crate::orchestration::{OrchestrationError, OrchestrationResult};
use std::collections::HashMap;

pub struct ResourcePool<T> {
    resource_type: ResourceType,
    available_resources: HashMap<String, T>,
    allocated_resources: HashMap<String, T>,
}

impl<T: Clone> ResourcePool<T> {
    pub fn new(resource_type: ResourceType) -> Self {
        Self {
            resource_type,
            available_resources: HashMap::new(),
            allocated_resources: HashMap::new(),
        }
    }

    pub fn add_resource(&mut self, resource: T) -> OrchestrationResult<()>
    where
        T: HasId,
    {
        let id = resource.get_id();
        self.available_resources.insert(id, resource);
        Ok(())
    }

    pub fn allocate_resource(&mut self, resource_id: &str) -> OrchestrationResult<T> {
        let resource = self
            .available_resources
            .remove(resource_id)
            .ok_or_else(|| {
                OrchestrationError::ResourceAllocationError(format!(
                    "Resource not available: {resource_id}"
                ))
            })?;

        self.allocated_resources
            .insert(resource_id.to_string(), resource.clone());
        Ok(resource)
    }

    pub fn release_resource(&mut self, resource_id: &str) -> OrchestrationResult<()> {
        if let Some(resource) = self.allocated_resources.remove(resource_id) {
            self.available_resources
                .insert(resource_id.to_string(), resource);
        }
        Ok(())
    }

    pub fn get_available_resources(&self) -> Vec<&T> {
        self.available_resources.values().collect()
    }

    pub fn get_allocated_resources(&self) -> Vec<&T> {
        self.allocated_resources.values().collect()
    }

    pub fn available_count(&self) -> usize {
        self.available_resources.len()
    }

    pub fn allocated_count(&self) -> usize {
        self.allocated_resources.len()
    }

    pub fn total_count(&self) -> usize {
        self.available_resources.len() + self.allocated_resources.len()
    }

    pub fn is_available(&self, resource_id: &str) -> bool {
        self.available_resources.contains_key(resource_id)
    }

    pub fn is_allocated(&self, resource_id: &str) -> bool {
        self.allocated_resources.contains_key(resource_id)
    }

    pub fn resource_type(&self) -> &ResourceType {
        &self.resource_type
    }
}
