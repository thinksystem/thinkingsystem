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

use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::time::{sleep, Duration, Instant, Sleep};
use tokio_stream::Stream;
pin_project! {
    pub struct MockSource<T> {
        interval: Duration,
        item: T,
        #[pin]
        sleeper: Sleep,
    }
}
impl<T> MockSource<T> {
    pub fn new(interval: Duration, item: T) -> Self {
        MockSource {
            interval,
            item,
            sleeper: sleep(interval),
        }
    }
    pub fn with_interval(interval: Duration) -> Self
    where
        T: Default,
    {
        Self::new(interval, T::default())
    }
    pub fn interval(&self) -> Duration {
        self.interval
    }
    pub fn item(&self) -> &T {
        &self.item
    }
}
impl<T> Stream for MockSource<T>
where
    T: Clone,
{
    type Item = T;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        match this.sleeper.as_mut().poll(cx) {
            Poll::Ready(_) => {
                this.sleeper.as_mut().reset(Instant::now() + *this.interval);
                Poll::Ready(Some(this.item.clone()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::MAX, None)
    }
}
