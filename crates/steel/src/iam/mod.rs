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

pub mod core;
pub mod crypto;
#[cfg(feature = "surrealdb")]
pub mod db;
#[cfg(feature = "surrealdb")]
pub mod identity_orchestrator;
pub mod jwt;
pub mod vc;

pub use core::{DidDocument, Proof, Service, VerifiableCredential, VerificationMethod};
pub use crypto::{
    CryptoError, CryptoHash, CryptoHasher, CryptoKeyPair, CryptoSignature, DidCrypto,
    HashAlgorithm, SignatureAlgorithm,
};
#[cfg(feature = "surrealdb")]
pub use identity_orchestrator::IdentityProvider;
pub use jwt::{Claims, JwtManager, TokenError};
pub use vc::VcManager;
