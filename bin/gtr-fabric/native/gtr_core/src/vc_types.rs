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

use rustler::NifStruct;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, NifStruct)]
#[module = "GtrFabric.Steel.Proof"]
pub struct Proof {
    #[serde(rename = "type")]
    pub proof_type: String,
    pub created: String,
    #[serde(rename = "verificationMethod")]
    pub verification_method: String,
    #[serde(rename = "proofPurpose")]
    pub proof_purpose: String,
    #[serde(rename = "proofValue")]
    pub proof_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, NifStruct)]
#[module = "GtrFabric.Steel.VerifiableCredential"]
pub struct VerifiableCredential {
    #[serde(rename = "@context")]
    pub context: Vec<String>,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub types: Vec<String>,
    pub issuer: String,
    #[serde(rename = "issuanceDate")]
    pub issuance_date: String,
    #[serde(rename = "credentialSubject")]
    pub credential_subject: HashMap<String, String>,
    pub proof: Proof,
}


impl From<steel::iam::core::VerifiableCredential> for VerifiableCredential {
    fn from(vc: steel::iam::core::VerifiableCredential) -> Self {
        VerifiableCredential {
            context: vc.context,
            id: vc.id,
            types: vc.types,
            issuer: vc.issuer,
            issuance_date: vc.issuance_date,
            credential_subject: vc
                .credential_subject
                .into_iter()
                .map(|(k, v)| (k, v.to_string()))
                .collect(),
            proof: Proof {
                proof_type: vc.proof.proof_type,
                created: vc.proof.created,
                verification_method: vc.proof.verification_method,
                proof_purpose: vc.proof.proof_purpose,
                proof_value: vc.proof.proof_value,
            },
        }
    }
}
