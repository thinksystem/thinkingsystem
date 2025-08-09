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

use serde_json::{json, Value};
use steel::policy::ast::{evaluate, EvaluationContext, Expression};

#[test]
fn test_field_access() {
    let data = json!({
        "classification": "SalesData",
        "amount": 100
    });
    let context = EvaluationContext::new(data);

    assert_eq!(
        context.get_field("data.classification"),
        Some(&Value::String("SalesData".to_string()))
    );
    assert_eq!(
        context.get_field("classification"),
        Some(&Value::String("SalesData".to_string()))
    );
}

#[test]
fn test_equals_expression() {
    let data = json!({
        "classification": "SalesData"
    });
    let context = EvaluationContext::new(data);

    let expr = Expression::Equals(
        Box::new(Expression::Field("data.classification".to_string())),
        Box::new(Expression::Value(Value::String("SalesData".to_string()))),
    );

    let result = evaluate(&expr, &context);
    assert!(result.is_true());
}

#[test]
fn test_in_expression() {
    let data = json!({
        "classification": "SalesData"
    });
    let context = EvaluationContext::new(data);

    let expr = Expression::In(
        Box::new(Expression::Field("data.classification".to_string())),
        vec![
            Expression::Value(Value::String("SalesData".to_string())),
            Expression::Value(Value::String("InventoryData".to_string())),
        ],
    );

    let result = evaluate(&expr, &context);
    assert!(result.is_true());
}

#[test]
fn test_contains_expression() {
    let data = json!({
        "customer_pii": {
            "name": "John Doe"
        }
    });
    let context = EvaluationContext::new(data);

    let expr = Expression::Contains(
        Box::new(Expression::Field("data".to_string())),
        "customer_pii".to_string(),
    );

    let result = evaluate(&expr, &context);
    assert!(result.is_true());
}
