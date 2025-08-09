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

use crate::blocks::rules::BlockError;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::{Ipv4Addr, Ipv6Addr};
use url::Url;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub allowed_domains: HashSet<String>,
    pub block_internal_ips: bool,
    pub request_timeout_seconds: u64,
}
impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            allowed_domains: HashSet::new(),
            block_internal_ips: true,
            request_timeout_seconds: 10,
        }
    }
}
pub fn validate_url(url_str: &str, config: &SecurityConfig) -> Result<Url, BlockError> {
    let url = Url::parse(url_str)
        .map_err(|e| BlockError::SecurityViolation(format!("Invalid URL format: {e}")))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(BlockError::SecurityViolation(format!(
            "URL scheme '{}' is forbidden. Only HTTP/HTTPS are allowed.",
            url.scheme()
        )));
    }
    if let Some(host) = url.host_str() {
        if !config.allowed_domains.is_empty()
            && !config.allowed_domains.iter().any(|d| host.ends_with(d))
        {
            return Err(BlockError::SecurityViolation(format!(
                "Domain '{host}' is not in the list of allowed domains."
            )));
        }
    } else {
        return Err(BlockError::SecurityViolation(
            "URL must have a host.".to_string(),
        ));
    }
    if config.block_internal_ips {
        if let Some(host) = url.host() {
            match host {
                url::Host::Ipv4(ip) => {
                    if is_private_ipv4(ip) {
                        return Err(BlockError::SecurityViolation(format!(
                            "Access to private IPv4 address {ip} is forbidden."
                        )));
                    }
                }
                url::Host::Ipv6(ip) => {
                    if is_private_ipv6(ip) {
                        return Err(BlockError::SecurityViolation(format!(
                            "Access to private IPv6 address {ip} is forbidden."
                        )));
                    }
                }
                _ => {}
            }
        }
    }
    Ok(url)
}
fn is_private_ipv4(ip: Ipv4Addr) -> bool {
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.octets() == [169, 254, 169, 254]
}
fn is_private_ipv6(ip: Ipv6Addr) -> bool {
    ip.is_loopback() || ip.is_unspecified() || (ip.segments()[0] & 0xfe00) == 0xfc00
}
