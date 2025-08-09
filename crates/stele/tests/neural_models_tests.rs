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

use stele::memory::neural_models::statistical_test;

#[test]
fn test_chi_squared_statistical_accuracy() {
    let p_05 = 1.0 - libm::erf(libm::sqrt(3.841 / 2.0));
    let p_01 = 1.0 - libm::erf(libm::sqrt(6.635 / 2.0));

    assert!(
        (p_05 - 0.05).abs() < 0.01,
        "p-value for χ²=3.841 should be ~0.05, got {p_05:.6}"
    );
    assert!(
        (p_01 - 0.01).abs() < 0.005,
        "p-value for χ²=6.635 should be ~0.01, got {p_01:.6}"
    );

    let (p_test1, g_stat1) = statistical_test(5, 50, 20, 1000);
    assert!(
        p_test1 > 0.0 && p_test1 < 1.0,
        "statistical_test p-value should be valid, got {p_test1}"
    );
    assert!(
        g_stat1 >= 0.0,
        "G-statistic should be non-negative, got {g_stat1}"
    );

    println!("Statistical accuracy test passed:");
    println!("χ² = 3.841, p-value = {p_05:.6} (expected ~0.05)");
    println!("χ² = 6.635, p-value = {p_01:.6} (expected ~0.01)");
}
