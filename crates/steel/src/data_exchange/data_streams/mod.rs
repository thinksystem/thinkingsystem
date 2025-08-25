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
use futures::future::try_join_all;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::sync::Arc;
pub mod cloudevents;
pub mod combine;
pub mod event;
pub mod grpc;
pub mod kafka;
pub mod mock;
pub mod mqtt;
pub mod quic;
pub mod topics;

pub use cloudevents::{
    Event as CloudEvent, EventConfig, EventError, EventHandler as CloudEventHandler,
    IncomingEventData, Sink as CloudSink,
};
pub use combine::*;
pub use event::{
    Duration, Event, EventBuilder, EventHandler, EventHeader, EventManager, EventType, Location,
    Neo4jEventHandler,
};
pub use grpc::*;
pub use kafka::*;
pub use mock::*;
pub use mqtt::*;
pub use quic::*;
pub use topics::*;
#[derive(Debug)]
pub enum Error {
    InternalError(Box<dyn std::error::Error + Send + Sync>),
    Cancelled,
    CodecError(serde_json::Error),
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InternalError(e) => write!(f, "InternalError: {e}"),
            Error::Cancelled => write!(f, "Cancelled"),
            Error::CodecError(e) => write!(f, "CodecError: {e}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::InternalError(e) => Some(e.deref()),
            Error::Cancelled => None,
            Error::CodecError(e) => Some(e),
        }
    }
}
#[async_trait]
pub trait Sink<T, E> {
    async fn consume(&self, item: T) -> Result<(), E>
    where
        T: 'async_trait;
}
pub struct DrainSink<E>(std::marker::PhantomData<E>);
impl<E> Default for DrainSink<E> {
    fn default() -> DrainSink<E> {
        DrainSink(std::marker::PhantomData)
    }
}
#[async_trait]
impl<T, E> Sink<T, E> for DrainSink<E>
where
    T: Send,
    E: Sync,
{
    async fn consume(&self, _item: T) -> Result<(), E>
    where
        T: 'async_trait,
    {
        Ok(())
    }
}
pub struct MultiSink<T, E> {
    sinks: Vec<Box<dyn Sink<Arc<T>, E> + Send + Sync>>,
}
impl<T, E> MultiSink<T, E> {
    pub fn new(sinks: Vec<Box<dyn Sink<Arc<T>, E> + Send + Sync>>) -> Self {
        Self { sinks }
    }
}
#[async_trait]
impl<T, E> Sink<T, E> for MultiSink<T, E>
where
    T: Send + Sync,
    E: Send,
{
    async fn consume(&self, item: T) -> Result<(), E>
    where
        T: 'async_trait,
    {
        let shared_item = Arc::new(item);
        let fut_vec: Vec<_> = self
            .sinks
            .iter()
            .map(|sink| sink.consume(shared_item.clone()))
            .collect();
        let _ = try_join_all(fut_vec).await?;
        Ok(())
    }
}
pub struct LogSink {
    name: String,
}
impl LogSink {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}
#[async_trait]
impl<T, E> Sink<T, E> for LogSink
where
    T: Send + Sync + std::fmt::Debug,
    E: Send,
{
    async fn consume(&self, item: T) -> Result<(), E>
    where
        T: 'async_trait,
    {
        println!("{}: {:?}", self.name, item);
        Ok(())
    }
}
#[async_trait]
pub trait Ack {
    async fn ack(&self) -> Result<(), Error>;
}
