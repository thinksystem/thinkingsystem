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

use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub name: String,
    pub iat: i64,
    pub exp: i64,
    pub iss: String,
    pub aud: String,
    pub did: Option<String>,
    pub roles: Vec<String>,
}

pub struct JwtManager {
    secret: String,
    issuer: String,
    audience: String,
}

impl Clone for JwtManager {
    fn clone(&self) -> Self {
        Self {
            secret: self.secret.clone(),
            issuer: self.issuer.clone(),
            audience: self.audience.clone(),
        }
    }
}

impl std::fmt::Debug for JwtManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtManager")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

impl JwtManager {
    pub fn new(secret: &str, issuer: String, audience: String) -> Self {
        Self {
            secret: secret.to_string(),
            issuer,
            audience,
        }
    }

    pub fn create_token(
        &self,
        user_id: &str,
        email: &str,
        name: &str,
        did: Option<String>,
        roles: Vec<String>,
        expires_in_hours: i64,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let now = Utc::now();
        let exp = now + Duration::hours(expires_in_hours);

        let claims = Claims {
            sub: user_id.to_string(),
            email: email.to_string(),
            name: name.to_string(),
            iat: now.timestamp(),
            exp: exp.timestamp(),
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            did,
            roles,
        };

        let header = Header::new(Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(self.secret.as_ref());
        encode(&header, &claims, &encoding_key)
    }

    pub fn verify_token(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.audience]);

        let decoding_key = DecodingKey::from_secret(self.secret.as_ref());
        let token_data = decode::<Claims>(token, &decoding_key, &validation)?;
        Ok(token_data.claims)
    }

    pub fn refresh_token(
        &self,
        token: &str,
        expires_in_hours: i64,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let claims = self.verify_token(token)?;

        self.create_token(
            &claims.sub,
            &claims.email,
            &claims.name,
            claims.did,
            claims.roles,
            expires_in_hours,
        )
    }

    pub fn extract_user_id(&self, token: &str) -> Result<String, jsonwebtoken::errors::Error> {
        let claims = self.verify_token(token)?;
        Ok(claims.sub)
    }

    pub fn has_role(
        &self,
        token: &str,
        required_role: &str,
    ) -> Result<bool, jsonwebtoken::errors::Error> {
        let claims = self.verify_token(token)?;
        Ok(claims.roles.contains(&required_role.to_string()))
    }

    pub fn has_any_role(
        &self,
        token: &str,
        required_roles: &[&str],
    ) -> Result<bool, jsonwebtoken::errors::Error> {
        let claims = self.verify_token(token)?;
        let user_roles: HashSet<String> = claims.roles.into_iter().collect();
        let required_set: HashSet<String> = required_roles.iter().map(|s| s.to_string()).collect();

        Ok(!user_roles.is_disjoint(&required_set))
    }
}

#[derive(Debug)]
pub enum TokenError {
    InvalidToken,
    ExpiredToken,
    InsufficientPermissions,
    MissingRole(String),
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::InvalidToken => write!(f, "Invalid token"),
            TokenError::ExpiredToken => write!(f, "Token has expired"),
            TokenError::InsufficientPermissions => write!(f, "Insufficient permissions"),
            TokenError::MissingRole(role) => write!(f, "Missing required role: {role}"),
        }
    }
}

impl std::error::Error for TokenError {}

impl From<jsonwebtoken::errors::Error> for TokenError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        match err.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => TokenError::ExpiredToken,
            _ => TokenError::InvalidToken,
        }
    }
}
