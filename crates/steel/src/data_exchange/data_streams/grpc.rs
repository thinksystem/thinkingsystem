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

use async_trait::async_trait;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::time::Duration;
use tonic::transport::Channel;
#[derive(Debug, Clone, Default)]
pub struct MyRequest {
    pub operator_id: String,
    pub package: String,
    pub data: String,
}
#[derive(Debug, Clone, Default)]
pub struct MyResponse {
    pub message: String,
    pub status: String,
}
#[derive(Debug, Clone)]
pub struct MyServiceClient<T> {
    _channel: T,
}
impl<T> MyServiceClient<T> {
    pub fn new(channel: T) -> Self {
        Self { _channel: channel }
    }
    pub async fn my_method(
        &mut self,
        request: tonic::Request<MyRequest>,
    ) -> Result<tonic::Response<MyResponse>, tonic::Status> {
        let req = request.into_inner();
        let response = MyResponse {
            message: format!("Processed: {}", req.data),
            status: "success".to_string(),
        };
        Ok(tonic::Response::new(response))
    }
}
#[derive(Debug)]
pub enum GrpcError {
    ConnectionError(String),
    RequestError(String),
    ResponseParseError(String),
    InvalidInput(String),
}
impl fmt::Display for GrpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GrpcError::ConnectionError(msg) => write!(f, "Connection error: {msg}"),
            GrpcError::RequestError(msg) => write!(f, "Request error: {msg}"),
            GrpcError::ResponseParseError(msg) => write!(f, "Response parse error: {msg}"),
            GrpcError::InvalidInput(msg) => write!(f, "Invalid input: {msg}"),
        }
    }
}
impl Error for GrpcError {}
impl From<tonic::Status> for GrpcError {
    fn from(status: tonic::Status) -> Self {
        GrpcError::RequestError(status.to_string())
    }
}
impl From<tonic::transport::Error> for GrpcError {
    fn from(err: tonic::transport::Error) -> Self {
        GrpcError::ConnectionError(err.to_string())
    }
}
#[derive(Debug, Clone)]
pub struct GrpcApiRequest {
    pub operator_id: String,
    pub package: String,
    pub data: String,
}
#[derive(Debug, Clone)]
pub struct GrpcApiResponse {
    pub message: String,
    pub status: String,
}
#[async_trait]
pub trait GrpcApiClient: Send + Sync + Clone {
    async fn call_service(&mut self, req: &GrpcApiRequest) -> Result<GrpcApiResponse, GrpcError>;
}
#[derive(Clone)]
pub struct GrpcApiClientImpl {
    client: MyServiceClient<Channel>,
}
impl GrpcApiClientImpl {
    pub fn new(channel: Channel) -> Self {
        let client = MyServiceClient::new(channel);
        Self { client }
    }
}
#[async_trait]
impl GrpcApiClient for GrpcApiClientImpl {
    async fn call_service(&mut self, req: &GrpcApiRequest) -> Result<GrpcApiResponse, GrpcError> {
        let grpc_request = MyRequest {
            operator_id: req.operator_id.clone(),
            package: req.package.clone(),
            data: req.data.clone(),
        };
        let response = self
            .client
            .my_method(tonic::Request::new(grpc_request))
            .await?;
        let grpc_response = response.into_inner();
        Ok(GrpcApiResponse {
            message: grpc_response.message,
            status: grpc_response.status,
        })
    }
}
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub grpc_address: String,
    pub timeout: Option<Duration>,
    pub max_retries: Option<u32>,
}
impl ConnectionInfo {
    pub fn new(grpc_address: String) -> Self {
        Self {
            grpc_address,
            timeout: None,
            max_retries: None,
        }
    }
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = Some(max_retries);
        self
    }
}
#[async_trait]
pub trait DataExchange<T, R> {
    async fn call(&self, operator_id: String, package: String, data: T) -> Result<R, GrpcError>;
}
pub struct GrpcDataExchange<C>
where
    C: GrpcApiClient,
{
    client: C,
    connection_info: ConnectionInfo,
}
impl<C> GrpcDataExchange<C>
where
    C: GrpcApiClient,
{
    pub fn new(connection_info: ConnectionInfo, client: C) -> Self {
        Self {
            client,
            connection_info,
        }
    }
    pub fn connection_info(&self) -> &ConnectionInfo {
        &self.connection_info
    }
}
#[async_trait]
impl<C> DataExchange<String, HashMap<String, String>> for GrpcDataExchange<C>
where
    C: GrpcApiClient,
{
    async fn call(
        &self,
        operator_id: String,
        package: String,
        data: String,
    ) -> Result<HashMap<String, String>, GrpcError> {
        if data.trim().is_empty() {
            return Err(GrpcError::InvalidInput("Data cannot be empty".to_string()));
        }
        let req = GrpcApiRequest {
            operator_id: operator_id.clone(),
            package: package.clone(),
            data,
        };
        let mut client_clone = self.client.clone();
        let response = client_clone.call_service(&req).await?;
        let mut result = HashMap::new();
        result.insert("operator_id".to_string(), operator_id);
        result.insert("package".to_string(), package);
        result.insert("response".to_string(), response.message);
        result.insert("status".to_string(), response.status);
        Ok(result)
    }
}
pub async fn create_grpc_client(
    connection_info: &ConnectionInfo,
) -> Result<GrpcApiClientImpl, GrpcError> {
    let mut endpoint = Channel::from_shared(connection_info.grpc_address.clone())
        .map_err(|e| GrpcError::ConnectionError(e.to_string()))?;
    if let Some(timeout) = connection_info.timeout {
        endpoint = endpoint.timeout(timeout);
    }
    let channel = endpoint.connect().await?;
    Ok(GrpcApiClientImpl::new(channel))
}
pub async fn create_grpc_data_exchange(
    grpc_address: String,
) -> Result<GrpcDataExchange<GrpcApiClientImpl>, GrpcError> {
    let connection_info = ConnectionInfo::new(grpc_address);
    let client = create_grpc_client(&connection_info).await?;
    Ok(GrpcDataExchange::new(connection_info, client))
}
pub async fn create_grpc_client_with_config(
    connection_info: &ConnectionInfo,
    configure_endpoint: impl FnOnce(tonic::transport::Endpoint) -> tonic::transport::Endpoint,
) -> Result<GrpcApiClientImpl, GrpcError> {
    let endpoint = Channel::from_shared(connection_info.grpc_address.clone())
        .map_err(|e| GrpcError::ConnectionError(e.to_string()))?;
    let configured_endpoint = configure_endpoint(endpoint);
    let channel = configured_endpoint.connect().await?;
    Ok(GrpcApiClientImpl::new(channel))
}
