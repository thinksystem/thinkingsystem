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

use thiserror::Error;

#[derive(Error, Debug)]
pub enum IAMError {
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("JWT error: {0}")]
    JwtError(String),

    #[error("Verifiable credential error: {0}")]
    VCError(String),

    #[error("User signup failed: {0}")]
    SignupFailed(String),

    #[error("User signin failed: {0}")]
    SigninFailed(String),

    #[error("Insufficient permissions: {0}")]
    InsufficientPermissions(String),

    #[error("User not found: {0}")]
    UserNotFound(String),
}

impl From<surrealdb::Error> for IAMError {
    fn from(err: surrealdb::Error) -> Self {
        IAMError::DatabaseError(err.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for IAMError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        IAMError::JwtError(err.to_string())
    }
}
