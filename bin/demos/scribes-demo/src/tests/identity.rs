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

use crate::identity::EnhancedIdentityVerifier;
use serde_json::json;
use std::sync::Arc;
use tracing::debug;

pub async fn test_identity_specialist(
    enhanced_verifier: &Arc<EnhancedIdentityVerifier>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    debug!("Testing Enhanced IdentityVerifier (legacy interface)");
    let context_to_verify = json!({"source_id": "urn:stele:log:1138"});
    let mut identities_verified = 0;
    if let Ok(result) = enhanced_verifier.verify_source(&context_to_verify).await {
        debug!("verify_source result: {}", result);
        identities_verified += 1;
    }
    let link_context = json!({"source_id": "id_1", "target_id": "id_2"});
    if let Ok(result) = enhanced_verifier.link_identities(&link_context).await {
        debug!("link_identities result: {}", result);
    }
    Ok(json!({ "identities_verified": identities_verified }))
}
