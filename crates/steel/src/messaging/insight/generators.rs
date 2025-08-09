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

use rand::distributions::Alphanumeric;
use rand::prelude::*;

pub fn generate_random_email() -> String {
    let mut rng = thread_rng();
    let username: String = (0..8).map(|_| rng.sample(Alphanumeric) as char).collect();
    let domain: String = (0..8).map(|_| rng.sample(Alphanumeric) as char).collect();
    format!("{}@{}.com", username.to_lowercase(), domain.to_lowercase())
}

pub fn generate_random_phone() -> String {
    let mut rng = thread_rng();
    format!(
        "{:03}-{:02}-{:04}",
        rng.gen_range(100..1000),
        rng.gen_range(10..100),
        rng.gen_range(1000..10000)
    )
}

pub fn generate_random_uuid() -> String {
    format!("urn:uuid:{}", uuid::Uuid::new_v4())
}

pub fn generate_random_api_key() -> String {
    let mut rng = thread_rng();
    (0..24).map(|_| rng.sample(Alphanumeric) as char).collect()
}

pub fn generate_random_credit_card() -> String {
    let mut rng = thread_rng();
    (0..16)
        .map(|_| rng.gen_range(b'0'..=b'9') as char)
        .collect()
}

pub fn generate_random_date() -> String {
    let mut rng = thread_rng();
    format!(
        "{:04}-{:02}-{:02}",
        rng.gen_range(1980..2030),
        rng.gen_range(1..13),
        rng.gen_range(1..29)
    )
}

pub fn generate_random_normal_word() -> String {
    let words = ["hello", "world", "message", "system", "process"];
    let mut rng = thread_rng();
    words[rng.gen_range(0..words.len())].to_string()
}
