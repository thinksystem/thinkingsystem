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

use crate::vc_types;
use rustler::NifResult;
use std::collections::HashMap;
use steel::iam::jwt::JwtManager;
use steel::iam::vc::VcManager;

#[rustler::nif]
pub fn create_trust_score_credential_nif(
    subject_did: String,
    trust_score: f64,
    performance_summary: HashMap<String, String>,
    issuer_token: String,
) -> NifResult<vc_types::VerifiableCredential> {
    let jwt_manager = JwtManager::new(
        "your_jwt_secret",
        "did:steel:issuer".to_string(),
        "gtr-fabric-consumer".to_string(),
    );
    let vc_manager = VcManager::new(jwt_manager, "did:steel:issuer".to_string());

    let perf_summary_json: HashMap<String, serde_json::Value> = performance_summary
        .into_iter()
        .map(|(k, v)| (k, serde_json::json!(v)))
        .collect();

    let result = vc_manager.create_trust_score_credential(
        &subject_did,
        trust_score,
        &perf_summary_json,
        &issuer_token,
    );

    match result {
        Ok(steel_vc) => Ok(vc_types::VerifiableCredential::from(steel_vc)),
        Err(e) => Err(rustler::Error::Term(Box::new(format!(
            "Failed to create VC: {e}"
        )))),
    }
}
