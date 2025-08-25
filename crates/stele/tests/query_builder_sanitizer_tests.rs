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



use stele::database::query_builder::{Condition, Operator, RelateQuery, SelectQuery, ReturnType};
use serde_json::json;

#[test]
fn select_query_sanitizes_fields_and_sources() {
    let q = SelectQuery::new()
        .fields(vec!["name".into(), "count(*)".into(), "bad;field".into()])
        .omit_fields(vec!["secret;drop".into()])
        .from(vec!["users".into(), "accounts;DROP".into()])
        .where_condition(Condition::simple("name", Operator::Contains, json!("a'b")))
        .order_by_asc("name;--".into())
        .fetch_fields(vec!["related->edges".into(), "x; y".into()]);

    let s = q.to_string();
    
    assert!(!s.contains(';') || s.ends_with(';'));
    
    assert!(s.contains(" FROM users, accountsDROP "));
    
    assert!(s.contains(" ORDER BY name-- ASC"));
    
    assert!(s.contains("SELECT name, count(*), badfield"));
    
    assert!(s.contains("WHERE name CONTAINS 'a\\'b'"));
}

#[test]
fn relate_query_sanitizes_parts() {
    let r = RelateQuery::new(
        "nodes:1; DROP".into(),
        "edges;DELETE".into(),
        "nodes:2|inject".into(),
    )
    .set("note;bad", json!("x"))
    .return_type(ReturnType::Fields(vec!["id".into(), "content;rm".into()]));
    let s = r.to_string();
    assert!(s.starts_with("RELATE "));
    assert!(s.contains(" nodes:1DROP->edgesDELETE->nodes:2inject"));
    assert!(s.contains(" SET notebad = 'x'"));
    assert!(s.contains(" RETURN id, contentrm"));
}

#[test]
fn raw_condition_validation_blocks_dangerous_keywords() {
    assert!(Condition::validated_raw("foo = 1").is_ok());
    assert!(Condition::validated_raw("name @@ 'text' AND depth <= 2").is_ok());
    assert!(Condition::validated_raw("DROP TABLE x").is_err());
    assert!(Condition::validated_raw("DELETE FROM y").is_err());
    
    assert!(Condition::validated_raw("name = ☃").is_err());
}
