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


use stele::nlu::llm_processor::LLMAdapter;


pub async fn classify_recoverable_error(
    adapter: &dyn LLMAdapter,
    error_text: &str,
) -> bool {
    let system = r#"You are a strict JSON classifier. Output only: {"recoverable": true|false}
Recoverable means: a small source edit (type fix, cast, missing import, minor precedence) can likely fix it.
Non-recoverable: architectural mismatch, missing symbol export by design, incompatible ABI, linker toolchain failure.
No extra keys, no commentary."#;
    let user = format!("Error:\n{error_text}\nDecide.");
    if let Ok(v) = adapter.generate_structured_response(system, &user).await {
        return v.get("recoverable").and_then(|b| b.as_bool()).unwrap_or(false);
    }
    false
}


pub async fn heal_rust_code(
    adapter: &dyn LLMAdapter,
    src: &str,
    error_text: &str,
) -> Option<String> {
    let system = r#"You output ONLY strict JSON: {"code": "<fixed_rust_source>"}
Goal: minimally edit the provided Rust source to resolve the given compiler/linker error.
Rules:
- Keep the same public surface (function names/exports) unless error demands a tiny change.
- Do not add external crates. Avoid large refactors. Prefer type casts, corrected precedence, add missing returns, fix borrow/mutability.
- Return only JSON with 'code'. No markdown fences, no commentary."#;
    let user = format!(
        "Error:\n{error_text}\n---\nSource:\n{src}\n---\nReturn fixed JSON now."
    );
    if let Ok(v) = adapter.generate_structured_response(system, &user).await {
        if let Some(code) = v.get("code").and_then(|s| s.as_str()) {
            return Some(code.to_string());
        }
    }
    None
}


#[allow(dead_code)]
pub async fn classify_prime_count(
    adapter: &dyn LLMAdapter,
    directive: &str,
) -> bool {
    let system = r#"Output ONLY JSON: {"prime_count": true|false}
The question: does the directive ask to count the number of prime numbers up to some bound?
Answer false if it's about finding particular primes, clusters, gaps, or best n, but not the count.
No commentary."#;
    let user = format!("Directive: {directive}\nDecide.");
    if let Ok(v) = adapter.generate_structured_response(system, &user).await {
        return v.get("prime_count").and_then(|b| b.as_bool()).unwrap_or(false);
    }
    false
}
