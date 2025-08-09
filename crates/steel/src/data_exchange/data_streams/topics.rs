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

pub const INPUT_TOPIC: &str = "input-topic";
pub const PRE_PROCESSING_TOPIC: &str = "pre-processing-topic";
pub const INFERENCE_TOPIC_PREFIX: &str = "inference-topic";
pub const DELEGATE_TOPIC_PREFIX: &str = "delegate-topic";
pub const AGENT_TOPIC: &str = "agent-topic";
pub const GRAPH_TOPIC_PREFIX: &str = "graph-topic";
pub const SESSION_TOPIC_PREFIX: &str = "session-topic";
pub const POST_PROCESSING_TOPIC: &str = "post-processing-topic";
pub const RESULT_TOPIC: &str = "result-topic";
pub const MONITORING_TOPIC: &str = "monitoring-topic";
pub const LOGGING_TOPIC: &str = "logging-topic";
pub const CONFIGURATION_TOPIC: &str = "configuration-topic";
pub fn dynamic_topic(topic_prefix: &str, topic_suffix: &str) -> String {
    format!("{topic_prefix}-{topic_suffix}")
}
