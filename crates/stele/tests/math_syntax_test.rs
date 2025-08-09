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

use surrealdb::{engine::remote::ws::Ws, opt::auth::Root, Surreal};

#[tokio::test]
async fn test_math_syntax() -> Result<(), Box<dyn std::error::Error>> {
    let db = Surreal::new::<Ws>("127.0.0.1:8000").await?;
    db.signin(Root {
        username: "root",
        password: "root",
    })
    .await?;
    db.use_ns("test").use_db("test").await?;

    let mut response = db.query("RETURN math::sqrt(16)").await?;
    let result: Option<f64> = response.take(0)?;
    println!("math::sqrt(16) = {result:?}");
    assert_eq!(result, Some(4.0));

    let mut response = db.query("RETURN math::pow(2, 3)").await?;
    let result: Option<f64> = response.take(0)?;
    println!("math::pow(2, 3) = {result:?}");
    assert_eq!(result, Some(8.0));

    db.query("CREATE scribes:test SET coords = [3.0, 4.0]")
        .await?;

    let mut response = db.query("SELECT math::sqrt(math::pow(coords[0] - 0, 2) + math::pow(coords[1] - 0, 2)) AS distance FROM scribes:test").await?;
    let result: Vec<serde_json::Value> = response.take(0)?;
    println!("Distance calculation result: {result:?}");

    db.query("DELETE scribes:test").await?;

    Ok(())
}
