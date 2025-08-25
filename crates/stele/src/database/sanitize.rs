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




pub fn sanitize_table_name(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if (c.is_ascii_alphanumeric() || c == '_') && (i > 0 || c.is_ascii_alphabetic() || c == '_') {
            out.push(c);
        }
    }
    if out.is_empty() { "_".to_string() } else { out }
}


pub fn sanitize_record_id(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':'))
        .collect()
}


pub fn sanitize_field_expr(s: &str) -> String {
    s.chars()
        .filter(|c| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    '_' | '.'
                        | '$'
                        | '@'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '('
                        | ')'
                        | '*'
                        | ' '
                        | ':'
                        | '-'
                        | '>'
                        | '<'
                        | '|'
                        | ','
                )
        })
        .collect()
}
