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

use steel::iam::identity_orchestrator::identity_provider::IdentityProvider;

#[tokio::test]
async fn test_identity_provider_signup_signin() {
    let provider = IdentityProvider::new()
        .await
        .expect("Failed to create identity provider");

    let token = provider
        .signup("John Doe", "john@example.com", "password123")
        .await
        .expect("Signup failed");

    assert!(!token.is_empty());

    let signin_token = provider
        .signin("john@example.com", "password123")
        .await
        .expect("Signin failed");

    assert!(!signin_token.is_empty());
}
