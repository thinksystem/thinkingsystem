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

use futures_core::Stream;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
pin_project! {
    pub struct TakeUntil<S, F>
    where
        S: Stream,
        F: Future,
    {
        #[pin]
        stream: Option<S>,
        #[pin]
        terminator: Option<F>,
        terminated: bool,
    }
}
impl<S, F> TakeUntil<S, F>
where
    S: Stream,
    F: Future,
{
    pub fn new(stream: S, terminator: F) -> Self {
        Self {
            stream: Some(stream),
            terminator: Some(terminator),
            terminated: false,
        }
    }
    pub fn is_terminated(&self) -> bool {
        self.terminated
    }
    pub fn into_inner(self) -> Option<S> {
        self.stream
    }
}
impl<S, F> Stream for TakeUntil<S, F>
where
    S: Stream,
    F: Future,
{
    type Item = S::Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        if *this.terminated {
            return Poll::Ready(None);
        }
        if let Some(terminator) = this.terminator.as_mut().as_pin_mut() {
            if terminator.poll(cx).is_ready() {
                *this.terminated = true;
                this.terminator.set(None);
                this.stream.set(None);
                return Poll::Ready(None);
            }
        }
        match this.stream.as_mut().as_pin_mut() {
            Some(stream) => match stream.poll_next(cx) {
                Poll::Ready(Some(item)) => Poll::Ready(Some(item)),
                Poll::Ready(None) => {
                    *this.terminated = true;
                    this.stream.set(None);
                    this.terminator.set(None);
                    Poll::Ready(None)
                }
                Poll::Pending => Poll::Pending,
            },
            None => Poll::Ready(None),
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.stream {
            Some(stream) => {
                let (lower, upper) = stream.size_hint();
                (lower, upper)
            }
            None => (0, Some(0)),
        }
    }
}
pin_project! {
    pub struct Merge<S1, S2>
    where
        S1: Stream,
        S2: Stream<Item = S1::Item>,
    {
        #[pin]
        stream1: Option<S1>,
        #[pin]
        stream2: Option<S2>,
    }
}
impl<S1, S2> Merge<S1, S2>
where
    S1: Stream,
    S2: Stream<Item = S1::Item>,
{
    pub fn new(stream1: S1, stream2: S2) -> Self {
        Self {
            stream1: Some(stream1),
            stream2: Some(stream2),
        }
    }
    pub fn is_terminated(&self) -> bool {
        self.stream1.is_none() && self.stream2.is_none()
    }
}
impl<S1, S2> Stream for Merge<S1, S2>
where
    S1: Stream,
    S2: Stream<Item = S1::Item>,
{
    type Item = S1::Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        if let Some(stream1) = this.stream1.as_mut().as_pin_mut() {
            match stream1.poll_next(cx) {
                Poll::Ready(Some(item)) => return Poll::Ready(Some(item)),
                Poll::Ready(None) => {
                    this.stream1.set(None);
                }
                Poll::Pending => {}
            }
        }
        if let Some(stream2) = this.stream2.as_mut().as_pin_mut() {
            match stream2.poll_next(cx) {
                Poll::Ready(Some(item)) => return Poll::Ready(Some(item)),
                Poll::Ready(None) => {
                    this.stream2.set(None);
                }
                Poll::Pending => {}
            }
        }
        if this.stream1.is_none() && this.stream2.is_none() {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower1, upper1) = self
            .stream1
            .as_ref()
            .map_or((0, Some(0)), |s| s.size_hint());
        let (lower2, upper2) = self
            .stream2
            .as_ref()
            .map_or((0, Some(0)), |s| s.size_hint());
        let lower = lower1.saturating_add(lower2);
        let upper = match (upper1, upper2) {
            (Some(u1), Some(u2)) => u1.checked_add(u2),
            _ => None,
        };
        (lower, upper)
    }
}
pub type Combine<S, F> = TakeUntil<S, F>;
impl<T, S, F> TakeUntil<S, F>
where
    S: Stream<Item = T>,
    F: Future<Output = ()>,
{
    #[deprecated(note = "Use TakeUntil::new instead")]
    pub fn combine(stream: S, task: F) -> Self {
        Self::new(stream, task)
    }
}
pub trait StreamExt: Stream {
    fn take_until<F>(self, terminator: F) -> TakeUntil<Self, F>
    where
        Self: Sized,
        F: Future,
    {
        TakeUntil::new(self, terminator)
    }
    fn merge<S2>(self, other: S2) -> Merge<Self, S2>
    where
        Self: Sized,
        S2: Stream<Item = Self::Item>,
    {
        Merge::new(self, other)
    }
}
impl<S: Stream> StreamExt for S {}
