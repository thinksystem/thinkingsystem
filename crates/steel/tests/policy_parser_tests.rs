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
use steel::policy::ast::{evaluate, EvaluationContext};
use steel::policy::parser::ConditionParser;

#[test]
fn test_parse_equals_expression() {
    let condition = "data.classification == 'SalesData'";
    let expr = ConditionParser::parse(condition).unwrap();

    let data = json!({ "classification": "SalesData" });
    let context = EvaluationContext::new(data);
    let result = evaluate(&expr, &context);

    assert!(result.is_true());
}

#[test]
fn test_parse_in_expression() {
    let condition = "data.classification in ['SalesData', 'InventoryData']";
    let expr = ConditionParser::parse(condition).unwrap();

    let data = json!({ "classification": "SalesData" });
    let context = EvaluationContext::new(data);
    let result = evaluate(&expr, &context);

    assert!(result.is_true());
}

#[test]
fn test_parse_contains_expression() {
    let condition = "data.contains('customer_pii')";
    let expr = ConditionParser::parse(condition).unwrap();

    let data = json!({ "customer_pii": { "name": "John" } });
    let context = EvaluationContext::new(data);
    let result = evaluate(&expr, &context);

    assert!(result.is_true());
}
