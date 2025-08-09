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

use serde_json::json;
use steel::policy::engine::{AuthorisationDecision, Policy, PolicyEngine};

fn create_test_policies() -> Vec<Policy> {
    vec![
        Policy {
            name: "allow_store_manager_sales".to_string(),
            role: "StoreManager".to_string(),
            action: "publish".to_string(),
            resource: "store_data_exchange".to_string(),
            effect: "allow".to_string(),
            conditions: vec!["data.classification == 'SalesData'".to_string()],
        },
        Policy {
            name: "deny_pii_data".to_string(),
            role: "any".to_string(),
            action: "publish".to_string(),
            resource: "analytics_service".to_string(),
            effect: "deny".to_string(),
            conditions: vec!["data.contains('customer_pii')".to_string()],
        },
    ]
}

#[test]
fn test_policy_engine_creation() {
    let policies = create_test_policies();
    let engine = PolicyEngine::new(policies).unwrap();
    let summary = engine.get_policy_summary();
    assert_eq!(summary.len(), 2);
}

#[test]
fn test_authorisation_allow() {
    let policies = create_test_policies();
    let engine = PolicyEngine::new(policies).unwrap();

    let data = json!({ "classification": "SalesData" });
    let user_roles = vec!["StoreManager".to_string()];

    let decision = engine.authorise(&user_roles, "publish", "store_data_exchange", &data);
    assert_eq!(decision, AuthorisationDecision::Allow);
}

#[test]
fn test_authorisation_explicit_deny() {
    let policies = create_test_policies();
    let engine = PolicyEngine::new(policies).unwrap();

    let data = json!({ "customer_pii": { "name": "John" } });
    let user_roles = vec!["RegionalAnalyst".to_string()];

    let decision = engine.authorise(&user_roles, "publish", "analytics_service", &data);
    match decision {
        AuthorisationDecision::Deny(_) => {}
        _ => panic!("Expected deny decision"),
    }
}

#[test]
fn test_authorisation_implicit_deny() {
    let policies = create_test_policies();
    let engine = PolicyEngine::new(policies).unwrap();

    let data = json!({ "classification": "InventoryData" });
    let user_roles = vec!["RegionalAnalyst".to_string()];

    let decision = engine.authorise(&user_roles, "publish", "store_data_exchange", &data);
    match decision {
        AuthorisationDecision::Deny(_) => {}
        _ => panic!("Expected deny decision"),
    }
}
