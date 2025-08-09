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

use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;

use crate::iam::core::{Proof, VerifiableCredential};
use crate::iam::crypto::{
    CryptoHash, CryptoHasher, CryptoKeyPair, CryptoSignature, SignatureAlgorithm,
};
use crate::iam::jwt::{Claims, JwtManager};

#[derive(Debug, Clone)]
pub struct VcManager {
    jwt_manager: JwtManager,
    issuer_did: String,
    issuer_keypair: CryptoKeyPair,
}

impl VcManager {
    pub fn new(jwt_manager: JwtManager, issuer_did: String) -> Self {
        let issuer_keypair = CryptoKeyPair::generate_ed25519();

        Self {
            jwt_manager,
            issuer_did,
            issuer_keypair,
        }
    }

    pub fn with_keypair(
        jwt_manager: JwtManager,
        issuer_did: String,
        issuer_keypair: CryptoKeyPair,
    ) -> Self {
        Self {
            jwt_manager,
            issuer_did,
            issuer_keypair,
        }
    }

    pub fn issuer_public_key(&self) -> String {
        self.issuer_keypair.public_key_base64()
    }

    fn create_credential_hash(&self, credential: &VerifiableCredential) -> CryptoHash {
        let mut hashable_credential = credential.clone();
        hashable_credential.proof = Proof {
            proof_type: "pending".to_string(),
            created: "pending".to_string(),
            verification_method: "pending".to_string(),
            proof_purpose: "pending".to_string(),
            proof_value: "pending".to_string(),
        };

        let credential_json =
            serde_json::to_string(&hashable_credential).unwrap_or_else(|_| "{}".to_string());

        CryptoHasher::sha512(credential_json.as_bytes())
    }

    fn create_ed25519_proof(
        &self,
        credential_hash: &CryptoHash,
        issuer_token: &str,
    ) -> Result<Proof, Box<dyn std::error::Error>> {
        let claims = self.jwt_manager.verify_token(issuer_token)?;
        if !claims.roles.contains(&"issuer".to_string())
            && !claims.roles.contains(&"admin".to_string())
        {
            return Err("Insufficient permissions: issuer or admin role required".into());
        }

        let now = Utc::now().to_rfc3339();

        let mut sign_data = Vec::new();
        sign_data.extend_from_slice(&credential_hash.hash_bytes);
        sign_data.extend_from_slice(now.as_bytes());
        sign_data.extend_from_slice(self.issuer_did.as_bytes());

        let signature = self.issuer_keypair.sign(&sign_data);

        Ok(Proof {
            proof_type: "Ed25519Signature2020".to_string(),
            created: now,
            verification_method: format!("{}#key-1", self.issuer_did),
            proof_purpose: "assertionMethod".to_string(),
            proof_value: signature.to_base64(),
        })
    }

    pub fn create_identity_credential(
        &self,
        subject_did: &str,
        subject_name: &str,
        subject_email: &str,
        issuer_token: &str,
    ) -> Result<VerifiableCredential, Box<dyn std::error::Error>> {
        let credential_id = format!("urn:uuid:{}", uuid::Uuid::new_v4());
        let now = Utc::now().to_rfc3339();

        let mut credential_subject = HashMap::new();
        credential_subject.insert("id".to_string(), json!(subject_did));
        credential_subject.insert("name".to_string(), json!(subject_name));
        credential_subject.insert("email".to_string(), json!(subject_email));

        let mut credential = VerifiableCredential {
            context: vec![
                "https://www.w3.org/2018/credentials/v1".to_string(),
                "https://steel.identity/credentials/v1".to_string(),
            ],
            id: Some(credential_id),
            types: vec![
                "VerifiableCredential".to_string(),
                "IdentityCredential".to_string(),
            ],
            issuer: self.issuer_did.clone(),
            issuance_date: now,
            credential_subject,
            proof: Proof {
                proof_type: "pending".to_string(),
                created: "pending".to_string(),
                verification_method: "pending".to_string(),
                proof_purpose: "pending".to_string(),
                proof_value: "pending".to_string(),
            },
        };

        let credential_hash = self.create_credential_hash(&credential);
        let proof = self.create_ed25519_proof(&credential_hash, issuer_token)?;

        credential.proof = proof;

        Ok(credential)
    }

    pub fn create_role_credential(
        &self,
        subject_did: &str,
        subject_name: &str,
        roles: Vec<String>,
        issuer_token: &str,
    ) -> Result<VerifiableCredential, Box<dyn std::error::Error>> {
        let credential_id = format!("urn:uuid:{}", uuid::Uuid::new_v4());
        let now = Utc::now().to_rfc3339();

        let mut credential_subject = HashMap::new();
        credential_subject.insert("id".to_string(), json!(subject_did));
        credential_subject.insert("name".to_string(), json!(subject_name));
        credential_subject.insert("roles".to_string(), json!(roles));

        let mut credential = VerifiableCredential {
            context: vec![
                "https://www.w3.org/2018/credentials/v1".to_string(),
                "https://steel.identity/credentials/v1".to_string(),
            ],
            id: Some(credential_id),
            types: vec![
                "VerifiableCredential".to_string(),
                "RoleCredential".to_string(),
            ],
            issuer: self.issuer_did.clone(),
            issuance_date: now,
            credential_subject,
            proof: Proof {
                proof_type: "pending".to_string(),
                created: "pending".to_string(),
                verification_method: "pending".to_string(),
                proof_purpose: "pending".to_string(),
                proof_value: "pending".to_string(),
            },
        };

        let credential_hash = self.create_credential_hash(&credential);
        let proof = self.create_ed25519_proof(&credential_hash, issuer_token)?;

        credential.proof = proof;

        Ok(credential)
    }

    pub fn create_solana_did_credential(
        &self,
        subject_did: &str,
        solana_public_key: &str,
        issuer_token: &str,
    ) -> Result<VerifiableCredential, Box<dyn std::error::Error>> {
        let credential_id = format!("urn:uuid:{}", uuid::Uuid::new_v4());
        let now = Utc::now().to_rfc3339();

        let mut credential_subject = HashMap::new();
        credential_subject.insert("id".to_string(), json!(subject_did));
        credential_subject.insert("solanaPublicKey".to_string(), json!(solana_public_key));

        let mut credential = VerifiableCredential {
            context: vec![
                "https://www.w3.org/2018/credentials/v1".to_string(),
                "https://steel.identity/credentials/v1".to_string(),
            ],
            id: Some(credential_id),
            types: vec![
                "VerifiableCredential".to_string(),
                "SolanaDidCredential".to_string(),
            ],
            issuer: self.issuer_did.clone(),
            issuance_date: now,
            credential_subject,
            proof: Proof {
                proof_type: "pending".to_string(),
                created: "pending".to_string(),
                verification_method: "pending".to_string(),
                proof_purpose: "pending".to_string(),
                proof_value: "pending".to_string(),
            },
        };

        let credential_hash = self.create_credential_hash(&credential);
        let proof = self.create_ed25519_proof(&credential_hash, issuer_token)?;

        credential.proof = proof;

        Ok(credential)
    }

    pub fn create_trust_score_credential(
        &self,
        subject_did: &str,
        trust_score: f64,
        performance_summary: &HashMap<String, serde_json::Value>,
        issuer_token: &str,
    ) -> Result<VerifiableCredential, Box<dyn std::error::Error>> {
        let credential_id = format!("urn:uuid:{}", uuid::Uuid::new_v4());
        let now = Utc::now().to_rfc3339();

        let mut credential_subject = HashMap::new();
        credential_subject.insert("id".to_string(), json!(subject_did));
        credential_subject.insert("trustScore".to_string(), json!(trust_score));
        credential_subject.insert(
            "trustAlgorithm".to_string(),
            json!("GtrFabric.Reputation.Heuristics.v1"),
        );
        credential_subject.insert("performanceSummary".to_string(), json!(performance_summary));

        let mut credential = VerifiableCredential {
            context: vec![
                "https://www.w3.org/2018/credentials/v1".to_string(),
                "https://steel.identity/credentials/v1".to_string(),
            ],
            id: Some(credential_id),
            types: vec![
                "VerifiableCredential".to_string(),
                "TrustScoreCredential".to_string(),
            ],
            issuer: self.issuer_did.clone(),
            issuance_date: now,
            credential_subject,
            proof: Proof {
                proof_type: "pending".to_string(),
                created: "pending".to_string(),
                verification_method: "pending".to_string(),
                proof_purpose: "pending".to_string(),
                proof_value: "pending".to_string(),
            },
        };

        let credential_hash = self.create_credential_hash(&credential);
        let proof = self.create_ed25519_proof(&credential_hash, issuer_token)?;

        credential.proof = proof;

        Ok(credential)
    }

    pub fn verify_credential(
        &self,
        credential: &VerifiableCredential,
    ) -> Result<Claims, Box<dyn std::error::Error>> {
        if credential.proof.proof_type == "Ed25519Signature2020" {
            return self.verify_ed25519_credential(credential);
        }

        if credential.proof.proof_type == "JsonWebTokenProof2020" {
            let claims = self
                .jwt_manager
                .verify_token(&credential.proof.proof_value)?;

            if credential.issuer != self.issuer_did {
                return Err("Credential issuer does not match expected issuer".into());
            }

            return Ok(claims);
        }

        Err("Unsupported proof type".into())
    }

    fn verify_ed25519_credential(
        &self,
        credential: &VerifiableCredential,
    ) -> Result<Claims, Box<dyn std::error::Error>> {
        let credential_hash = self.create_credential_hash(credential);

        let mut sign_data = Vec::new();
        sign_data.extend_from_slice(&credential_hash.hash_bytes);
        sign_data.extend_from_slice(credential.proof.created.as_bytes());
        sign_data.extend_from_slice(credential.issuer.as_bytes());

        let signature = CryptoSignature::from_base64(
            &credential.proof.proof_value,
            SignatureAlgorithm::Ed25519,
        )?;

        self.issuer_keypair.verify(&sign_data, &signature)?;

        let subject = credential
            .credential_subject
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let name = credential
            .credential_subject
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");

        let email = credential
            .credential_subject
            .get("email")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown@example.com");

        let roles = credential
            .credential_subject
            .get("roles")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_else(|| vec!["verified".to_string()]);

        Ok(Claims {
            sub: subject.to_string(),
            email: email.to_string(),
            name: name.to_string(),
            iat: chrono::Utc::now().timestamp(),
            exp: chrono::Utc::now().timestamp() + (24 * 60 * 60),
            iss: credential.issuer.clone(),
            aud: "steel-iam".to_string(),
            did: Some(subject.to_string()),
            roles,
        })
    }

    pub fn extract_roles(
        &self,
        credential: &VerifiableCredential,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        self.verify_credential(credential)?;

        if let Some(roles_value) = credential.credential_subject.get("roles") {
            if let Some(roles_array) = roles_value.as_array() {
                let roles: Vec<String> = roles_array
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect();
                return Ok(roles);
            }
        }

        Err("No roles found in credential".into())
    }

    pub fn get_credential_metadata(
        &self,
        credential: &VerifiableCredential,
    ) -> HashMap<String, serde_json::Value> {
        let mut metadata = HashMap::new();

        metadata.insert("id".to_string(), json!(credential.id));
        metadata.insert("issuer".to_string(), json!(credential.issuer));
        metadata.insert("types".to_string(), json!(credential.types));
        metadata.insert("issuance_date".to_string(), json!(credential.issuance_date));

        metadata.insert("proof_type".to_string(), json!(credential.proof.proof_type));
        metadata.insert(
            "verification_method".to_string(),
            json!(credential.proof.verification_method),
        );
        metadata.insert(
            "proof_purpose".to_string(),
            json!(credential.proof.proof_purpose),
        );

        if credential.proof.proof_type == "Ed25519Signature2020" {
            let credential_hash = self.create_credential_hash(credential);
            metadata.insert(
                "credential_hash".to_string(),
                json!(credential_hash.to_hex()),
            );
            metadata.insert(
                "hash_algorithm".to_string(),
                json!(credential_hash.algorithm.to_string()),
            );
            metadata.insert("signature_algorithm".to_string(), json!("Ed25519"));
            metadata.insert("public_key".to_string(), json!(self.issuer_public_key()));
        }

        metadata
    }
}
