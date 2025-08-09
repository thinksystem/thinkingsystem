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

use std::fs;
use std::path::Path;

pub async fn init_schema(
    db: &surrealdb::Surreal<impl surrealdb::Connection>,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/iam/db/schema.surql");

    let schema_content = fs::read_to_string(schema_path)?;

    db.query(schema_content).await?;

    Ok(())
}

pub fn get_schema_content() -> &'static str {
    include_str!("schema.surql")
}
