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

pub mod entity_network;
pub mod learning_system;
pub mod llm_monitor;
pub mod message_display;
pub mod scribe_window;
pub mod startup_window;

pub use entity_network::EntityNetworkWindow;
pub use learning_system::LearningSystemWindow;
pub use llm_monitor::LLMMonitorWindow;
pub use scribe_window::ScribeWindow;
pub use startup_window::StartupWindow;
