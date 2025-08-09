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

use steel::iam::crypto::{
    CryptoHasher, CryptoKeyPair, DidCrypto, HashAlgorithm, SignatureAlgorithm,
};

#[test]
fn test_ed25519_keypair_generation() {
    let keypair = CryptoKeyPair::generate_ed25519();
    assert_eq!(keypair.algorithm, SignatureAlgorithm::Ed25519);
    assert_eq!(keypair.public_key_bytes().len(), 32);
}

#[test]
fn test_signature_verification() {
    let keypair = CryptoKeyPair::generate_ed25519();
    let data = b"test data to sign";

    let signature = keypair.sign(data);
    assert!(keypair.verify(data, &signature).is_ok());

    let wrong_data = b"wrong data";
    assert!(keypair.verify(wrong_data, &signature).is_err());
}

#[test]
fn test_hash_functions() {
    let data = b"test data";

    let sha512_hash = CryptoHasher::sha512(data);
    assert_eq!(sha512_hash.algorithm, HashAlgorithm::Sha512);
    assert_eq!(sha512_hash.hash_bytes.len(), 64);

    let sha3_hash = CryptoHasher::sha3_512(data);
    assert_eq!(sha3_hash.algorithm, HashAlgorithm::Sha3_512);
    assert_eq!(sha3_hash.hash_bytes.len(), 64);

    let blake3_hash = CryptoHasher::blake3(data);
    assert_eq!(blake3_hash.algorithm, HashAlgorithm::Blake3);
    assert_eq!(blake3_hash.hash_bytes.len(), 32);
}

#[test]
fn test_did_generation() {
    let (did, keypair) = DidCrypto::generate_did("steel");
    assert!(did.starts_with("did:steel:"));

    let public_key = DidCrypto::public_key_from_did(&did).unwrap();
    assert_eq!(public_key.as_bytes(), &keypair.public_key_bytes()[..]);
}

#[test]
fn test_did_signature_verification() {
    let (did, keypair) = DidCrypto::generate_did("steel");
    let data = b"test message";
    let signature = keypair.sign(data);

    assert!(DidCrypto::verify_did_signature(&did, data, &signature).is_ok());
}
