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


use std::sync::atomic::{AtomicU64, Ordering};

static HYBRID_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static HYBRID_SUCCESS: AtomicU64 = AtomicU64::new(0);
static HYBRID_FALLBACK: AtomicU64 = AtomicU64::new(0);
static HYBRID_VIRTUAL_NODE: AtomicU64 = AtomicU64::new(0);

pub fn record_attempt() {
    HYBRID_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
}
pub fn record_success() {
    HYBRID_SUCCESS.fetch_add(1, Ordering::Relaxed);
}
pub fn record_fallback() {
    HYBRID_FALLBACK.fetch_add(1, Ordering::Relaxed);
}
pub fn record_virtual_node() {
    HYBRID_VIRTUAL_NODE.fetch_add(1, Ordering::Relaxed);
}
pub fn hybrid_metrics() -> (u64, u64, u64, u64) {
    (
        HYBRID_ATTEMPTS.load(Ordering::Relaxed),
        HYBRID_SUCCESS.load(Ordering::Relaxed),
        HYBRID_FALLBACK.load(Ordering::Relaxed),
        HYBRID_VIRTUAL_NODE.load(Ordering::Relaxed),
    )
}
