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

pub mod assembly;
pub mod dependency;
pub mod executor;
pub mod function;
pub mod hot_reload;
pub mod import_export;
pub mod metrics;
pub mod strategy;
pub use assembly::*;
pub use dependency::DependencyManager;
pub use executor::{DynamicExecutor, DynamicSource};
pub use function::DynamicFunction;
pub use hot_reload::HotReloadManager;
pub use import_export::ImportExportManager;
pub use metrics::PerformanceMetrics;
pub use strategy::*;
