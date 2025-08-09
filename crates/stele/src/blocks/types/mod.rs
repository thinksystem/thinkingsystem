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

pub mod api_explorer;
pub mod compute;
pub mod conditional;
pub mod decision;
pub mod display;
pub mod external_data;
pub mod goto;
pub mod input;
pub mod interactive;
pub mod llm_blocks;
pub mod random;
pub mod terminal;
pub use api_explorer::{APIExplorerBlock, DataExchangeInterface};
pub use compute::ComputeBlock;
pub use conditional::ConditionalBlock;
pub use decision::DecisionBlock;
pub use display::DisplayBlock;
pub use external_data::ExternalDataBlock;
pub use goto::GoToBlock;
pub use input::InputBlock;
pub use interactive::InteractiveBlock;
pub use llm_blocks::{
    IntelligentDecisionBlock, LLMContentAnalyserBlock, LLMContentGeneratorBlock, LLMInterface,
    OutputAggregatorBlock, StandardProcessorBlock,
};
pub use random::RandomBlock;
pub use terminal::TerminalBlock;
