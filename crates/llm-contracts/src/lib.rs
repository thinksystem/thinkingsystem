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

pub mod config;
pub mod requests;
pub mod responses;
pub mod types;

pub use config::{
    AuthenticationConfig, CircuitBreakerState, CostPerMillionTokens, CostTier as V1CostTier,
    FeedbackConfig, IntentWeights, ModelConfig, ModelDefinition, ProviderConfig, RateLimits,
    SelectionStrategy, SpeedTier as V1SpeedTier,
};
pub use requests::*;
pub use responses::*;
pub use types::{Capability, CostTier, LLMError, LLMResult, Provider, SpeedTier};
