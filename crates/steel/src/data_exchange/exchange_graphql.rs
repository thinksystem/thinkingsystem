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

use crate::data_exchange::error::DataExchangeError;
use crate::data_exchange::exchange_core::{DataExchangeConfig, DataExchangeProcessor};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
pub trait PaymentProvider: Send + Sync {
    fn process_payment(
        &self,
        amount: f64,
        currency: &str,
        payment_details: HashMap<String, String>,
    ) -> Result<String, String>;
}
pub struct MockPaymentProvider;
impl PaymentProvider for MockPaymentProvider {
    fn process_payment(
        &self,
        amount: f64,
        currency: &str,
        _payment_details: HashMap<String, String>,
    ) -> Result<String, String> {
        Ok(format!("mock-tx-{amount}-{currency}"))
    }
}
pub struct ApiContext {
    pub data_exchange_processor: Arc<DataExchangeProcessor>,
    pub payment_providers: HashMap<String, Arc<dyn PaymentProvider>>,
}
#[async_trait]
pub trait ApiContextFactory: Send + Sync {
    async fn create_context(&self) -> Result<ApiContext, DataExchangeError>;
}
pub struct AppContextFactory {
    config: DataExchangeConfig,
}
impl AppContextFactory {
    pub fn new(config: DataExchangeConfig) -> Result<Self, DataExchangeError> {
        Ok(Self { config })
    }
}
#[async_trait]
impl ApiContextFactory for AppContextFactory {
    async fn create_context(&self) -> Result<ApiContext, DataExchangeError> {
        let data_exchange_processor = DataExchangeProcessor::new(&self.config).await?;
        let mut payment_providers = HashMap::new();
        payment_providers.insert(
            "mock".to_string(),
            Arc::new(MockPaymentProvider) as Arc<dyn PaymentProvider>,
        );
        Ok(ApiContext {
            data_exchange_processor: Arc::new(data_exchange_processor),
            payment_providers,
        })
    }
}
pub struct DataExchangeApi {
    context_factory: Arc<dyn ApiContextFactory>,
}
impl DataExchangeApi {
    pub fn new(context_factory: Arc<dyn ApiContextFactory>) -> Self {
        Self { context_factory }
    }
    pub async fn data_exchange(
        &self,
        request: DataExchangeRequest,
    ) -> Result<DataExchangeResponse, DataExchangeError> {
        let context = self.context_factory.create_context().await?;
        let result = context
            .data_exchange_processor
            .exchange_data(&request.provider_name, request.request)
            .await?;
        Ok(DataExchangeResponse { result })
    }
    pub async fn process_payment(
        &self,
        request: PaymentRequest,
    ) -> Result<PaymentResponse, DataExchangeError> {
        let context = self.context_factory.create_context().await?;
        let provider = context
            .payment_providers
            .get(&request.provider_name)
            .ok_or_else(|| DataExchangeError::ProviderNotFound(request.provider_name.clone()))?;
        let transaction_id = provider
            .process_payment(request.amount, &request.currency, request.payment_details)
            .map_err(DataExchangeError::Configuration)?;
        Ok(PaymentResponse {
            transaction_id,
            status: "success".to_string(),
        })
    }
}
pub fn create_api_with_context(
    config: DataExchangeConfig,
) -> Result<DataExchangeApi, DataExchangeError> {
    let context_factory = AppContextFactory::new(config)?;
    let api = DataExchangeApi::new(Arc::new(context_factory) as Arc<dyn ApiContextFactory>);
    Ok(api)
}
#[derive(Debug, Serialize, Deserialize)]
pub struct DataExchangeRequest {
    pub provider_name: String,
    pub request: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct DataExchangeResponse {
    pub result: HashMap<String, String>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PaymentRequest {
    pub provider_name: String,
    pub amount: f64,
    pub currency: String,
    pub payment_details: HashMap<String, String>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PaymentResponse {
    pub transaction_id: String,
    pub status: String,
}
#[cfg(feature = "http-server")]
pub mod http_handlers {
    use super::*;
    use std::sync::Arc;
    pub struct HttpServer {
        api: Arc<DataExchangeApi>,
    }
    impl HttpServer {
        pub fn new(api: DataExchangeApi) -> Self {
            Self { api: Arc::new(api) }
        }
        pub async fn handle_data_exchange(
            &self,
            request: DataExchangeRequest,
        ) -> Result<DataExchangeResponse, String> {
            self.api
                .data_exchange(request)
                .await
                .map_err(|e| e.to_string())
        }
        pub async fn handle_payment(
            &self,
            request: PaymentRequest,
        ) -> Result<PaymentResponse, String> {
            self.api
                .process_payment(request)
                .await
                .map_err(|e| e.to_string())
        }
    }
}
