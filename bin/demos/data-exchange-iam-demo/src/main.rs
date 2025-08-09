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

#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_assignments)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use steel::data_exchange::{
    ConnectionType, DataExchangeImpl, HttpDataExchangeImpl, MessageMetadata, ProviderConfig,
};
use steel::{AuthorisationDecision, IdentityProvider, PolicyEngine, PolicyLoader};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

struct DataExchangeService {
    policy_engine: PolicyEngine,
    iam_provider: Arc<IdentityProvider>,
    http_implementations: HashMap<String, HttpDataExchangeImpl>,
}

impl DataExchangeService {
    async fn new(
        policy_file_path: &str,
        iam_provider: Arc<IdentityProvider>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        info!("Loading policy configuration from {}", policy_file_path);

        let policy_engine = PolicyLoader::load_from_file(policy_file_path).await?;
        let provider_configs = PolicyLoader::load_providers_from_file(policy_file_path).await?;

        let providers: Vec<ProviderConfig> = provider_configs
            .iter()
            .map(|p| ProviderConfig {
                name: p.name.clone(),
                connection_type: match p.connection_type.as_str() {
                    "rest" => ConnectionType::Rest,
                    "grpc" => ConnectionType::Grpc,
                    "kafka" => ConnectionType::Kafka,
                    "mqtt" => ConnectionType::Mqtt,
                    _ => ConnectionType::Rest,
                },
                config: p.config.clone(),
            })
            .collect();

        let mut http_implementations = HashMap::new();
        for provider_config in &providers {
            if matches!(provider_config.connection_type, ConnectionType::Rest) {
                match HttpDataExchangeImpl::new(provider_config) {
                    Ok(impl_) => {
                        http_implementations.insert(provider_config.name.clone(), impl_);
                        info!("Created HTTP implementation for {}", provider_config.name);
                    }
                    Err(e) => {
                        warn!(
                            "Failed to create HTTP implementation for {}: {}",
                            provider_config.name, e
                        );
                    }
                }
            } else {
                warn!(
                    "Non-REST provider {} not supported yet",
                    provider_config.name
                );
            }
        }

        info!("Data exchange service initialised with policy engine");

        Ok(Self {
            policy_engine,
            iam_provider,
            http_implementations,
        })
    }

    #[instrument(skip(self, data), fields(provider = %provider_name, classification = %data.get("classification").unwrap_or(&serde_json::Value::String("unknown".to_string()))))]
    pub async fn publish_data(
        &self,
        user_token: &str,
        provider_name: &str,
        data: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let claims = self.iam_provider.verify_token(user_token).await?;
        let user_roles = &claims.roles;
        debug!(user_id = %claims.sub, roles = ?user_roles, "Authorising user publication attempt");

        let decision = self
            .policy_engine
            .authorise(user_roles, "publish", provider_name, &data);

        match decision {
            AuthorisationDecision::Allow => {
                debug!("Authorisation granted");
            }
            AuthorisationDecision::Deny(reason) => {
                error!("Authorisation denied: {}", reason);
                return Err(reason.into());
            }
        }

        if let Some(http_impl) = self.http_implementations.get(provider_name) {
            let mut metadata = MessageMetadata::default();
            if let Some(classification) = data.get("classification").and_then(|v| v.as_str()) {
                metadata = metadata.with_type(classification);
            }

            let payload = serde_json::to_string(&data)?;
            match http_impl.exchange_data(payload).await {
                Ok(response) => {
                    info!(
                        "Successfully published data to {} - Response: {:?}",
                        provider_name, response
                    );
                }
                Err(e) => {
                    info!(
                        "Attempted to publish to {} (demo endpoint): {}",
                        provider_name, e
                    );
                }
            }
        } else {
            return Err(format!("Provider '{provider_name}' not found or not supported").into());
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting Data Exchange IAM Integration Test with Policy Engine");
    let demo_start = Instant::now();

    let iam_provider = Arc::new(IdentityProvider::new().await?);
    info!(
        duration_ms = demo_start.elapsed().as_millis(),
        "IAM provider initialised"
    );

    let service_start = Instant::now();
    let data_exchange_service =
        DataExchangeService::new("policy.yaml", Arc::clone(&iam_provider)).await?;
    info!(
        duration_ms = service_start.elapsed().as_millis(),
        "Data exchange service initialised"
    );

    let users = create_demo_users(&iam_provider).await?;
    run_policy_scenarios(&data_exchange_service, &users).await;

    info!(
        duration_ms = demo_start.elapsed().as_millis(),
        "Data Exchange IAM test completed"
    );

    Ok(())
}

async fn create_demo_users(
    iam_provider: &IdentityProvider,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut users = HashMap::new();
    let admin_token = iam_provider
        .bootstrap_admin("Demo Admin", "admin@fusion.retail", "admin_password")
        .await?;
    info!("Admin user bootstrapped for role assignment.");

    let alice_email = format!("alice-{}@fusion.retail", Uuid::new_v4());
    iam_provider
        .signup("Alice Manager", &alice_email, "store_pass_123")
        .await?;
    iam_provider
        .assign_role(&alice_email, "StoreManager", &admin_token)
        .await?;
    let alice_token = iam_provider
        .create_token_with_database_roles(&alice_email, &admin_token)
        .await?;
    users.insert("store_manager".to_string(), alice_token);

    let bob_email = format!("bob-{}@fusion.retail", Uuid::new_v4());
    iam_provider
        .signup("Bob Coordinator", &bob_email, "logistics_pass_123")
        .await?;
    iam_provider
        .assign_role(&bob_email, "LogisticsCoordinator", &admin_token)
        .await?;
    let bob_token = iam_provider
        .create_token_with_database_roles(&bob_email, &admin_token)
        .await?;
    users.insert("logistics_coordinator".to_string(), bob_token);

    let charlie_email = format!("charlie-{}@fusion.retail", Uuid::new_v4());
    iam_provider
        .signup("Charlie Analyst", &charlie_email, "analyst_pass_123")
        .await?;
    iam_provider
        .assign_role(&charlie_email, "RegionalAnalyst", &admin_token)
        .await?;
    let charlie_token = iam_provider
        .create_token_with_database_roles(&charlie_email, &admin_token)
        .await?;
    users.insert("regional_analyst".to_string(), charlie_token);

    info!("Demo users created and assigned roles.");
    Ok(users)
}

async fn run_policy_scenarios(
    data_exchange_service: &DataExchangeService,
    users: &HashMap<String, String>,
) {
    info!("Running Policy-Based Access Control Scenarios...");

    let sales_data = serde_json::json!({
        "classification": "SalesData",
        "store_id": "ST-01",
        "items": 3,
        "total": 75.50,
        "timestamp": chrono::Utc::now().to_rfc3339()
    });
    if let Some(token) = users.get("store_manager") {
        if let Err(e) = data_exchange_service
            .publish_data(token, "store_data_exchange", sales_data)
            .await
        {
            error!(
                "Scenario A [FAILED]: StoreManager was unexpectedly denied: {}",
                e
            );
        } else {
            info!("Scenario A [SUCCESS]: StoreManager published sales data as expected.");
        }
    }

    let logistics_command = serde_json::json!({
        "classification": "InventoryCommand",
        "command": "TRANSFER",
        "from_store": "ST-01",
        "to_warehouse": "WH-C",
        "sku": "FUS-WID-001",
        "quantity": 50
    });
    if let Some(token) = users.get("store_manager") {
        if let Err(e) = data_exchange_service
            .publish_data(token, "logistics_api", logistics_command)
            .await
        {
            info!(
                "Scenario B [SUCCESS]: StoreManager correctly denied from logistics API: {}",
                e
            );
        } else {
            warn!("Scenario B [FAILED]: StoreManager was unexpectedly allowed to access logistics API.");
        }
    }

    let logistics_command_bob = serde_json::json!({
        "classification": "InventoryCommand",
        "command": "RESTOCK",
        "warehouse": "WH-C",
        "sku": "FUS-GAD-007",
        "quantity": 200
    });
    if let Some(token) = users.get("logistics_coordinator") {
        if let Err(e) = data_exchange_service
            .publish_data(token, "logistics_api", logistics_command_bob)
            .await
        {
            error!(
                "Scenario C [FAILED]: LogisticsCoordinator was unexpectedly denied: {}",
                e
            );
        } else {
            info!("Scenario C [SUCCESS]: LogisticsCoordinator published inventory command as expected.");
        }
    }

    let pii_data = serde_json::json!({
        "classification": "AnalyticsQuery",
        "query_id": "q-123",
        "customer_pii": {
            "name": "John Doe",
            "email": "john.doe@example.com"
        }
    });
    if let Some(token) = users.get("regional_analyst") {
        if let Err(e) = data_exchange_service
            .publish_data(token, "analytics_service", pii_data)
            .await
        {
            info!(
                "Scenario D [SUCCESS]: Policy correctly blocked PII data publication: {}",
                e
            );
        } else {
            error!("Scenario D [CRITICAL FAILURE]: Policy allowed PII data to be published.");
        }
    }

    let clean_query = serde_json::json!({
        "classification": "AnalyticsQuery",
        "query_id": "q-456",
        "query": "SELECT COUNT(*) FROM sales WHERE date >= '2024-01-01'",
        "aggregated": true
    });
    if let Some(token) = users.get("regional_analyst") {
        if let Err(e) = data_exchange_service
            .publish_data(token, "analytics_service", clean_query)
            .await
        {
            error!(
                "Scenario E [FAILED]: Analyst was unexpectedly denied for a clean query: {}",
                e
            );
        } else {
            info!("Scenario E [SUCCESS]: Analyst published clean analytics query as expected.");
        }
    }
}
