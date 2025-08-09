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

use crate::demo_processor::DemoDataProcessor;
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tracing::debug;

pub async fn test_data_specialist(
    enhanced_processor: &Arc<DemoDataProcessor>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    debug!("Testing Enhanced DataProcessor (legacy interface)");
    let test_context = json!({"text": "Some data to process.", "urgency": 0.5});
    let mut records_processed = 0;
    let start_time = Instant::now();
    if let Ok(result) = enhanced_processor.process_data(&test_context).await {
        debug!("process_data result: {}", result);
        if let Ok(storage_result) = enhanced_processor.store_extracted_data(&result).await {
            debug!("store_extracted_data result: {}", storage_result);
            records_processed += 1;
        }
    }
    let processing_time_ms = start_time.elapsed().as_millis();
    Ok(json!({ "records_processed": records_processed, "processing_time_ms": processing_time_ms }))
}
