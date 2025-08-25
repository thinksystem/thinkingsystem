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



use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};
use regex::Regex;

static LEGACY_COUNT: AtomicU64 = AtomicU64::new(0);
static OPAQUE_COUNT: AtomicU64 = AtomicU64::new(0);

static LEGACY_RE: Lazy<Regex> = Lazy::new(|| Regex::new("^[A-Za-z0-9_-]{3,}$").unwrap());
static OPAQUE_RE: Lazy<Regex> = Lazy::new(|| Regex::new("^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$").unwrap());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdKind { Legacy, Opaque, Unknown }

pub fn classify(id: &str) -> IdKind {
    if OPAQUE_RE.is_match(id) { IdKind::Opaque } else if LEGACY_RE.is_match(id) { IdKind::Legacy } else { IdKind::Unknown }
}

pub fn normalize(id: &str) -> &str {
    match classify(id) {
        IdKind::Opaque => { OPAQUE_COUNT.fetch_add(1, Ordering::Relaxed); id }
        IdKind::Legacy => {
            LEGACY_COUNT.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(id, "IdNormalizer: legacy id encountered");
            id
        }
        IdKind::Unknown => id,
    }
}

pub fn id_metrics() -> (u64,u64) { (LEGACY_COUNT.load(Ordering::Relaxed), OPAQUE_COUNT.load(Ordering::Relaxed)) }
