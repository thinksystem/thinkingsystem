#![cfg(feature = "surrealdb")]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use surrealdb::engine::any::{connect, Any};
use surrealdb::Surreal;

use crate::iam::db;
use crate::iam::jwt::{Claims, JwtManager};
use crate::iam::vc::VcManager;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AuthToken {
    token: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct UserRolesRecord {
    id: Option<surrealdb::sql::Thing>,
    user_email: String,
    roles: Vec<String>,
}

#[derive(Debug)]
pub struct IdentityProvider {
    db: Surreal<Any>,
    admin_db: Surreal<Any>,
    jwt_manager: JwtManager,
    vc_manager: VcManager,
}

impl IdentityProvider {
    pub async fn new() -> Result<Self, surrealdb::Error> {
        let db = connect("mem://").await?;
        db.use_ns("test").use_db("test").await?;

        let admin_db = connect("mem://").await?;
        admin_db.use_ns("test").use_db("test").await?;

        if let Err(e) = db::init_schema(&db).await {
            eprintln!("Warning: Failed to initialise schema: {e}");
        }

        let jwt_manager = JwtManager::new(
            "default_secret_key_change_in_production",
            "steel-iam".to_string(),
            "steel-users".to_string(),
        );

        let vc_manager = VcManager::new(jwt_manager.clone(), "did:steel:iam:issuer".to_string());

        Ok(Self {
            db,
            admin_db,
            jwt_manager,
            vc_manager,
        })
    }

    pub async fn new_with_connection(
        connection_string: &str,
        jwt_secret: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let db = connect(connection_string).await?;
        db.use_ns("production").use_db("identity").await?;

        let admin_db = connect(connection_string).await?;
        admin_db.use_ns("production").use_db("identity").await?;

        db::init_schema(&db).await?;

        let jwt_manager = JwtManager::new(
            jwt_secret,
            "steel-iam".to_string(),
            "steel-users".to_string(),
        );

        let vc_manager = VcManager::new(jwt_manager.clone(), "did:steel:iam:issuer".to_string());

        Ok(Self {
            db,
            admin_db,
            jwt_manager,
            vc_manager,
        })
    }

    pub async fn signup(
        &self,
        name: &str,
        email: &str,
        password: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        use surrealdb::opt::auth::Record;

        let result = self
            .db
            .signup(Record {
                namespace: "test",
                database: "test",
                access: "user",
                params: serde_json::json!({
                    "name": name,
                    "email": email,
                    "password": password
                }),
            })
            .await;

        match result {
            Ok(_) => {
                let user_id = format!("user:{email}");

                tracing::info!(
                    " Using email-based stable user ID for signup: {}",
                    user_id
                );

                let token = self.jwt_manager.create_token(
                    &user_id,
                    email,
                    name,
                    None,
                    vec!["user".to_string()],
                    24,
                )?;

                Ok(token)
            }
            Err(e) => Err(format!("Signup failed: {e}").into()),
        }
    }

    pub async fn signin(
        &self,
        email: &str,
        password: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        use surrealdb::opt::auth::Record;

        let result = self
            .db
            .signin(Record {
                namespace: "test",
                database: "test",
                access: "user",
                params: serde_json::json!({
                    "email": email,
                    "password": password
                }),
            })
            .await;

        match result {
            Ok(_) => {
                let user_id = format!("user:{email}");

                tracing::info!(
                    " Using email-based stable user ID for signin: {}",
                    user_id
                );

                let (name, did) = if let Ok(Some(user)) = self.get_user_by_email(email).await {
                    let name = user["name"].as_str().unwrap_or("User");
                    let did = user["solana_did"].as_str().map(|s| s.to_string());
                    (name.to_string(), did)
                } else {
                    ("User".to_string(), None)
                };

                let roles = self
                    .get_user_roles(email)
                    .await
                    .unwrap_or_else(|_| vec!["user".to_string()]);

                let token = self
                    .jwt_manager
                    .create_token(&user_id, email, &name, did, roles, 24)?;

                Ok(token)
            }
            Err(e) => Err(format!("Signin failed: {e}").into()),
        }
    }

    pub async fn add_solana_did(
        &self,
        user_email: &str,
        solana_did: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let user_email = user_email.to_string();
        let solana_did = solana_did.to_string();

        self.db
            .query("UPDATE user SET solana_did = $solana_did WHERE email = $email")
            .bind(("email", user_email))
            .bind(("solana_did", solana_did))
            .await?;

        Ok(())
    }

    pub async fn get_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<serde_json::Value>, Box<dyn std::error::Error>> {
        let email = email.to_string();

        let mut result = self
            .db
            .query("SELECT * FROM user WHERE email = $email")
            .bind(("email", email))
            .await?;

        let users: Vec<serde_json::Value> = result.take(0)?;
        Ok(users.into_iter().next())
    }

    pub async fn get_user_by_email_admin(
        &self,
        email: &str,
    ) -> Result<Option<serde_json::Value>, Box<dyn std::error::Error>> {
        let email = email.to_string();

        let mut result = self
            .admin_db
            .query("SELECT * FROM user WHERE email = $email")
            .bind(("email", email))
            .await?;

        let users: Vec<serde_json::Value> = result.take(0)?;
        Ok(users.into_iter().next())
    }

    pub async fn verify_token(&self, token: &str) -> Result<Claims, Box<dyn std::error::Error>> {
        let claims = self.jwt_manager.verify_token(token)?;
        Ok(claims)
    }

    pub async fn refresh_token(
        &self,
        token: &str,
        expires_in_hours: i64,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let new_token = self.jwt_manager.refresh_token(token, expires_in_hours)?;
        Ok(new_token)
    }

    pub async fn has_role(
        &self,
        token: &str,
        role: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let has_role = self.jwt_manager.has_role(token, role)?;
        Ok(has_role)
    }

    pub async fn bootstrap_admin(
        &self,
        admin_name: &str,
        admin_email: &str,
        admin_password: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        use surrealdb::opt::auth::Record;

        let result = self
            .db
            .signup(Record {
                namespace: "test",
                database: "test",
                access: "user",
                params: serde_json::json!({
                    "name": admin_name,
                    "email": admin_email,
                    "password": admin_password
                }),
            })
            .await;

        match result {
            Ok(_) => {
                tracing::info!("Admin user created, now assigning admin roles");

                let admin_roles = vec!["admin".to_string(), "user".to_string()];

                let _: Vec<UserRolesRecord> = self
                    .admin_db
                    .query("DELETE user_roles WHERE user_email = $email")
                    .bind(("email", admin_email.to_string()))
                    .await?
                    .take(0)?;

                let mut result = self
                    .admin_db
                    .query("CREATE user_roles SET user_email = $email, roles = $roles")
                    .bind(("email", admin_email.to_string()))
                    .bind(("roles", admin_roles.clone()))
                    .await?;

                let created_records: Vec<UserRolesRecord> = result.take(0)?;
                tracing::info!("Admin role assignment result: {:?}", created_records);

                let admin_id = format!("admin:{admin_email}");

                tracing::info!("Using email-based stable admin ID: {}", admin_id);

                let token = self.jwt_manager.create_token(
                    &admin_id,
                    admin_email,
                    admin_name,
                    None,
                    admin_roles,
                    24,
                )?;

                Ok(token)
            }
            Err(e) => Err(format!("Admin bootstrap failed: {e}").into()),
        }
    }

    pub async fn assign_role(
        &self,
        user_email: &str,
        role: &str,
        admin_token: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.jwt_manager.has_role(admin_token, "admin")? {
            return Err("Insufficient permissions: admin role required".into());
        }

        let mut updated_roles: Vec<String> = {
            let mut result = self
                .admin_db
                .query("SELECT roles FROM user_roles WHERE user_email = $email")
                .bind(("email", user_email.to_string()))
                .await?;

            let role_records: Vec<BTreeMap<String, serde_json::Value>> = result.take(0)?;

            if let Some(record) = role_records.first() {
                if let Some(roles_value) = record.get("roles") {
                    if let Some(roles_array) = roles_value.as_array() {
                        roles_array
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        };

        if !updated_roles.contains(&"user".to_string()) {
            updated_roles.push("user".to_string());
        }

        if !updated_roles.contains(&role.to_string()) {
            updated_roles.push(role.to_string());
        }

        tracing::info!(
            " Assigning roles to user: email={}, updated_roles={:?}",
            user_email,
            updated_roles
        );

        let _: Vec<UserRolesRecord> = self
            .admin_db
            .query("DELETE user_roles WHERE user_email = $email")
            .bind(("email", user_email.to_string()))
            .await?
            .take(0)?;

        let mut result = self
            .admin_db
            .query("CREATE user_roles SET user_email = $email, roles = $roles")
            .bind(("email", user_email.to_string()))
            .bind(("roles", updated_roles.clone()))
            .await?;

        let created_records: Vec<UserRolesRecord> = result.take(0)?;
        tracing::info!("Role assignment result: {:?}", created_records);

        Ok(())
    }

    pub async fn get_user_roles(
        &self,
        user_email: &str,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        tracing::info!("Getting roles for user: {}", user_email);

        let mut result = self
            .admin_db
            .query("SELECT roles FROM user_roles WHERE user_email = $email")
            .bind(("email", user_email.to_string()))
            .await?;

        let role_records: Vec<BTreeMap<String, serde_json::Value>> = result.take(0)?;
        tracing::info!("Raw role records: {:?}", role_records);

        if let Some(record) = role_records.first() {
            if let Some(roles_value) = record.get("roles") {
                tracing::info!("Roles value: {:?}", roles_value);
                if let Some(roles_array) = roles_value.as_array() {
                    let roles: Vec<String> = roles_array
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    tracing::info!("Parsed roles: {:?}", roles);
                    return Ok(roles);
                }
            }
        }

        tracing::warn!(
            " No roles found for user {}, defaulting to 'user'",
            user_email
        );
        Ok(vec!["user".to_string()])
    }

    pub async fn signin_with_roles(
        &self,
        email: &str,
        password: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        use surrealdb::opt::auth::Record;

        let result = self
            .db
            .signin(Record {
                namespace: "test",
                database: "test",
                access: "user",
                params: serde_json::json!({
                    "email": email,
                    "password": password
                }),
            })
            .await;

        match result {
            Ok(_) => {
                if let Ok(Some(user)) = self.get_user_by_email(email).await {
                    let user_id = user["id"].as_str().unwrap_or("unknown");
                    let name = user["name"].as_str().unwrap_or("Unknown User");
                    let did = user["solana_did"].as_str().map(|s| s.to_string());

                    let roles = self
                        .get_user_roles(email)
                        .await
                        .unwrap_or_else(|_| vec!["user".to_string()]);

                    let token = self
                        .jwt_manager
                        .create_token(user_id, email, name, did, roles, 24)?;

                    Ok(token)
                } else {
                    Err("User not found".into())
                }
            }
            Err(e) => Err(format!("Signin failed: {e}").into()),
        }
    }

    pub async fn create_token_with_database_roles(
        &self,
        user_email: &str,
        admin_token: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        if !self.jwt_manager.has_role(admin_token, "admin")? {
            return Err("Insufficient permissions: admin role required".into());
        }

        let roles = self.get_user_roles(user_email).await?;

        let user_id = format!("user:{user_email}");

        let token = self
            .jwt_manager
            .create_token(&user_id, user_email, "User", None, roles, 24)?;

        Ok(token)
    }

    pub async fn update_user_roles(
        &self,
        admin_token: &str,
        user_email: &str,
        new_roles: Vec<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.jwt_manager.has_role(admin_token, "admin")? {
            return Err("Insufficient permissions: admin role required".into());
        }

        let _: Vec<UserRolesRecord> = self
            .admin_db
            .query("DELETE user_roles WHERE user_email = $email")
            .bind(("email", user_email.to_string()))
            .await?
            .take(0)?;

        let mut result = self
            .admin_db
            .query("CREATE user_roles SET user_email = $email, roles = $roles")
            .bind(("email", user_email.to_string()))
            .bind(("roles", new_roles))
            .await?;

        let _created_records: Vec<UserRolesRecord> = result.take(0)?;

        Ok(())
    }

    pub async fn create_identity_credential(
        &self,
        subject_did: &str,
        subject_name: &str,
        subject_email: &str,
        issuer_token: &str,
    ) -> Result<crate::iam::core::VerifiableCredential, Box<dyn std::error::Error>> {
        self.vc_manager.create_identity_credential(
            subject_did,
            subject_name,
            subject_email,
            issuer_token,
        )
    }

    pub async fn create_role_credential(
        &self,
        subject_did: &str,
        subject_name: &str,
        roles: Vec<String>,
        issuer_token: &str,
    ) -> Result<crate::iam::core::VerifiableCredential, Box<dyn std::error::Error>> {
        self.vc_manager
            .create_role_credential(subject_did, subject_name, roles, issuer_token)
    }

    pub async fn create_solana_did_credential(
        &self,
        subject_did: &str,
        solana_public_key: &str,
        issuer_token: &str,
    ) -> Result<crate::iam::core::VerifiableCredential, Box<dyn std::error::Error>> {
        self.vc_manager
            .create_solana_did_credential(subject_did, solana_public_key, issuer_token)
    }

    pub async fn verify_credential(
        &self,
        credential: &crate::iam::core::VerifiableCredential,
    ) -> Result<Claims, Box<dyn std::error::Error>> {
        self.vc_manager.verify_credential(credential)
    }

    pub async fn extract_roles_from_credential(
        &self,
        credential: &crate::iam::core::VerifiableCredential,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        self.vc_manager.extract_roles(credential)
    }
}
