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

use base64::{engine::general_purpose, Engine as _};
use blake3::Hasher as Blake3Hasher;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use sha3::Sha3_512;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum HashAlgorithm {
    #[default]
    Sha512,
    Sha3_512,
    Blake3,
}

impl fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HashAlgorithm::Sha512 => write!(f, "SHA-512"),
            HashAlgorithm::Sha3_512 => write!(f, "SHA3-512"),
            HashAlgorithm::Blake3 => write!(f, "BLAKE3"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum SignatureAlgorithm {
    #[default]
    Ed25519,
}

impl fmt::Display for SignatureAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SignatureAlgorithm::Ed25519 => write!(f, "Ed25519"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CryptoKeyPair {
    pub(crate) signing_key: SigningKey,
    pub(crate) verifying_key: VerifyingKey,
    pub algorithm: SignatureAlgorithm,
}

impl CryptoKeyPair {
    pub fn generate_ed25519() -> Self {
        let mut csprng = OsRng;
        let mut signing_key_bytes = [0u8; 32];
        csprng.fill_bytes(&mut signing_key_bytes);
        let signing_key = SigningKey::from_bytes(&signing_key_bytes);
        let verifying_key = signing_key.verifying_key();

        Self {
            signing_key,
            verifying_key,
            algorithm: SignatureAlgorithm::Ed25519,
        }
    }

    pub fn public_key_bytes(&self) -> Vec<u8> {
        self.verifying_key.as_bytes().to_vec()
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(self.public_key_bytes())
    }

    pub fn public_key_base64(&self) -> String {
        general_purpose::STANDARD.encode(self.public_key_bytes())
    }

    pub fn to_did(&self, method: &str) -> String {
        match self.algorithm {
            SignatureAlgorithm::Ed25519 => {
                format!("did:{}:{}", method, self.public_key_base64())
            }
        }
    }

    pub fn sign(&self, data: &[u8]) -> CryptoSignature {
        match self.algorithm {
            SignatureAlgorithm::Ed25519 => {
                let signature = self.signing_key.sign(data);
                CryptoSignature {
                    algorithm: self.algorithm.clone(),
                    signature_bytes: signature.to_bytes().to_vec(),
                }
            }
        }
    }

    pub fn verify(&self, data: &[u8], signature: &CryptoSignature) -> Result<(), CryptoError> {
        if signature.algorithm != self.algorithm {
            return Err(CryptoError::AlgorithmMismatch);
        }

        match self.algorithm {
            SignatureAlgorithm::Ed25519 => {
                if signature.signature_bytes.len() != 64 {
                    return Err(CryptoError::InvalidSignature);
                }

                let mut sig_array = [0u8; 64];
                sig_array.copy_from_slice(&signature.signature_bytes);

                let signature_obj = Signature::from_bytes(&sig_array);
                self.verifying_key
                    .verify(data, &signature_obj)
                    .map_err(|_| CryptoError::SignatureVerificationFailed)
            }
        }
    }
}

impl Drop for CryptoKeyPair {
    fn drop(&mut self) {}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoSignature {
    pub algorithm: SignatureAlgorithm,
    pub signature_bytes: Vec<u8>,
}

impl CryptoSignature {
    pub fn to_hex(&self) -> String {
        hex::encode(&self.signature_bytes)
    }

    pub fn to_base64(&self) -> String {
        general_purpose::STANDARD.encode(&self.signature_bytes)
    }

    pub fn from_hex(hex_str: &str, algorithm: SignatureAlgorithm) -> Result<Self, CryptoError> {
        let signature_bytes = hex::decode(hex_str).map_err(|_| CryptoError::InvalidEncoding)?;

        Ok(Self {
            algorithm,
            signature_bytes,
        })
    }

    pub fn from_base64(b64_str: &str, algorithm: SignatureAlgorithm) -> Result<Self, CryptoError> {
        let signature_bytes = general_purpose::STANDARD
            .decode(b64_str)
            .map_err(|_| CryptoError::InvalidEncoding)?;

        Ok(Self {
            algorithm,
            signature_bytes,
        })
    }
}

pub struct CryptoHasher;

impl CryptoHasher {
    pub fn hash(data: &[u8], algorithm: Option<HashAlgorithm>) -> CryptoHash {
        let algorithm = algorithm.unwrap_or_default();

        let hash_bytes = match algorithm {
            HashAlgorithm::Sha512 => {
                let mut hasher = Sha512::new();
                hasher.update(data);
                hasher.finalize().to_vec()
            }
            HashAlgorithm::Sha3_512 => {
                let mut hasher = Sha3_512::new();
                hasher.update(data);
                hasher.finalize().to_vec()
            }
            HashAlgorithm::Blake3 => {
                let mut hasher = Blake3Hasher::new();
                hasher.update(data);
                hasher.finalize().as_bytes().to_vec()
            }
        };

        CryptoHash {
            algorithm,
            hash_bytes,
        }
    }

    pub fn sha512(data: &[u8]) -> CryptoHash {
        Self::hash(data, Some(HashAlgorithm::Sha512))
    }

    pub fn sha3_512(data: &[u8]) -> CryptoHash {
        Self::hash(data, Some(HashAlgorithm::Sha3_512))
    }

    pub fn blake3(data: &[u8]) -> CryptoHash {
        Self::hash(data, Some(HashAlgorithm::Blake3))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoHash {
    pub algorithm: HashAlgorithm,
    pub hash_bytes: Vec<u8>,
}

impl CryptoHash {
    pub fn to_hex(&self) -> String {
        hex::encode(&self.hash_bytes)
    }

    pub fn to_base64(&self) -> String {
        general_purpose::STANDARD.encode(&self.hash_bytes)
    }

    pub fn verify(&self, data: &[u8]) -> bool {
        let computed_hash = CryptoHasher::hash(data, Some(self.algorithm.clone()));
        self.hash_bytes == computed_hash.hash_bytes
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("Algorithm mismatch")]
    AlgorithmMismatch,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    #[error("Invalid encoding")]
    InvalidEncoding,

    #[error("Key generation failed")]
    KeyGenerationFailed,

    #[error("Hash computation failed")]
    HashComputationFailed,
}

pub struct DidCrypto;

impl DidCrypto {
    pub fn generate_did(method: &str) -> (String, CryptoKeyPair) {
        let keypair = CryptoKeyPair::generate_ed25519();
        let did = keypair.to_did(method);
        (did, keypair)
    }

    pub fn public_key_from_did(did: &str) -> Result<VerifyingKey, CryptoError> {
        if !did.starts_with("did:steel:") {
            return Err(CryptoError::InvalidEncoding);
        }

        let key_part = did
            .strip_prefix("did:steel:")
            .ok_or(CryptoError::InvalidEncoding)?;

        let key_bytes = general_purpose::STANDARD
            .decode(key_part)
            .map_err(|_| CryptoError::InvalidEncoding)?;

        if key_bytes.len() != 32 {
            return Err(CryptoError::InvalidEncoding);
        }

        let mut array = [0u8; 32];
        array.copy_from_slice(&key_bytes);

        VerifyingKey::from_bytes(&array).map_err(|_| CryptoError::InvalidEncoding)
    }

    pub fn verify_did_signature(
        did: &str,
        data: &[u8],
        signature: &CryptoSignature,
    ) -> Result<(), CryptoError> {
        let verifying_key = Self::public_key_from_did(did)?;

        if signature.algorithm != SignatureAlgorithm::Ed25519 {
            return Err(CryptoError::AlgorithmMismatch);
        }

        if signature.signature_bytes.len() != 64 {
            return Err(CryptoError::InvalidSignature);
        }

        let mut sig_array = [0u8; 64];
        sig_array.copy_from_slice(&signature.signature_bytes);

        let signature_obj = Signature::from_bytes(&sig_array);
        verifying_key
            .verify(data, &signature_obj)
            .map_err(|_| CryptoError::SignatureVerificationFailed)
    }
}
