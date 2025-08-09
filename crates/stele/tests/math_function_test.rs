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
async fn test_math_functions() -> Result<(), Box<dyn std::error::Error>> {
    let db = Surreal::new::<Ws>("127.0.0.1:8000").await?;
    db.signin(Root {
        username: "root",
        password: "root",
    })
    .await?;
    db.use_ns("test").use_db("test").await?;

    let create_result = db
        .query("CREATE scribes:test SET coords = [3.0, 4.0]")
        .await?;
    println!("Created test scribe: {create_result:?}");

    println!("Testing the exact query from scribe method...");
    let distance_query = "SELECT * FROM scribes ORDER BY math::sqrt(math::pow(coords[0] - 0.5, 2) + math::pow(coords[1] - 0.5, 2)) ASC LIMIT 1";
    let distance_result = db.query(distance_query).await;
    match distance_result {
        Ok(mut response) => {
            let result: Result<Vec<serde_json::Value>, _> = response.take(0);
            println!("Distance query result: {result:?}");
        }
        Err(e) => println!("Distance query failed: {e}"),
    }

    println!("Testing simpler math query...");
    let simple_query = "SELECT math::sqrt(25) as distance FROM scribes LIMIT 1";
    let simple_result = db.query(simple_query).await;
    match simple_result {
        Ok(mut response) => {
            let result: Result<Vec<serde_json::Value>, _> = response.take(0);
            println!("Simple math query result: {result:?}");
        }
        Err(e) => println!("Simple math query failed: {e}"),
    }

    db.query("DELETE scribes:test").await?;

    Ok(())
}
