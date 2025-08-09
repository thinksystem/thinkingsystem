#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2024 Jonathan Lee
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License version 3
# as published by the Free Software Foundation.
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
# See the GNU Affero General Public License for more details.
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see https://www.gnu.org/licenses/.

"""
Test suite for the chart renderer module.
Comprehensive testing for all chart types and edge cases.
"""

import json
import sys
import time
import traceback
from typing import Dict, Any, List, Tuple

# Import all functions from the main renderer module
from renderer import (
    render_chart,
    create_temp_html_chart,
    validate_chart_mappings,
    create_sample_data,
    get_available_charts,
    _create_figure,
    _get_base_layout,
    _create_error_figure
)

def run_test(test_name: str, chart_name: str, data: Dict[str, Any], mappings: Dict[str, str]) -> bool:
    """Run a single test case."""
    print(f"\n--- {test_name} ---")
    try:
        validation = validate_chart_mappings(chart_name, mappings, data)
        if validation['errors']:
            print(f"  ❌ Validation errors: {validation['errors']}")
            return False
        if validation['warnings']:
            print(f"  ⚠️  Validation warnings: {validation['warnings']}")
        
        html_path = create_temp_html_chart(chart_name, json.dumps(data), mappings)
        print(f"  ✅ Success: {html_path}")
        return True
    except Exception as e:
        print(f"  ❌ Failed: {e}")
        return False

def get_test_cases() -> List[Tuple[str, str, Dict[str, Any], Dict[str, str]]]:
    """Get all test cases."""
    sample_data = {"x_axis": [1, 2, 3], "y_axis": [10, 15, 13], "category": ["A", "B", "A"]}
    problematic_data = {
        "gross_mthly_75_percentile": ["4000", "2900", "3500", "4100", "na", "3365", "3800", "na"],
        "school": ["School A", "School B", "School C", "School D", "School E", "School F", "School G", "School H"]
    }
    
    return [
        ("Basic Scatter", "scatter", sample_data, {"x": "x_axis", "y": "y_axis", "color": "category"}),
        ("Basic Bar", "bar", {"categories": ["A", "B", "C"], "values": [10, 15, 13]}, {"x": "categories", "y": "values"}),
        ("Problematic Treemap", "treemap", problematic_data, {"values": "gross_mthly_75_percentile", "names": "school"}),
        ("Problematic Sunburst", "sunburst", problematic_data, {"values": "gross_mthly_75_percentile", "names": "school"}),
        ("Line Chart", "line", sample_data, {"x": "x_axis", "y": "y_axis"}),
        ("Pie Chart", "pie", {"names": ["A", "B", "C"], "values": [10, 15, 13]}, {"names": "names", "values": "values"}),
        ("Doughnut Chart", "doughnut", {"names": ["A", "B", "C"], "values": [10, 15, 13]}, {"names": "names", "values": "values"}),
        ("Histogram", "histogram", sample_data, {"x": "y_axis"}),
        ("Box Plot", "box", {"category": ["A", "A", "B", "B", "C", "C"], "values": [10, 12, 15, 18, 13, 16]}, {"x": "category", "y": "values"}),
        ("Violin Plot", "violin", {"category": ["A", "A", "B", "B", "C", "C"], "values": [10, 12, 15, 18, 13, 16]}, {"x": "category", "y": "values"}),
        ("Surface Plot", "surface", create_sample_data("surface"), {"x": "x", "y": "y", "z": "z"}),
        ("Candlestick", "candlestick", create_sample_data("candlestick"), {"x": "date", "open": "open", "high": "high", "low": "low", "close": "close"}),
        ("Sankey", "sankey", create_sample_data("sankey"), {"source": "source", "target": "target", "value": "value"}),
        ("Indicator", "indicator", create_sample_data("indicator"), {"value": "metric"}),
        ("Waterfall", "waterfall", {"x": ["Start", "Q1", "Q2", "Q3", "Q4"], "y": [100, 20, -10, 15, -5], "measure": ["absolute", "relative", "relative", "relative", "relative"]}, {"x": "x", "y": "y", "measure": "measure"}),
        ("Scatter Matrix", "scatter_matrix", {"var1": [1, 2, 3, 4, 5], "var2": [10, 20, 15, 25, 30], "var3": [5, 8, 12, 7, 9]}, {}),
        ("Parallel Categories", "parallel_categories", {"cat1": ["A", "B", "A", "B", "A"], "cat2": ["X", "Y", "X", "Y", "X"], "cat3": ["1", "2", "1", "2", "1"]}, {}),
        ("Parallel Coordinates", "parallel_coordinates", {"var1": [1, 2, 3, 4, 5], "var2": [10, 20, 15, 25, 30], "var3": [5, 8, 12, 7, 9]}, {}),
    ]

def run_basic_tests():
    """Run all basic chart tests."""
    print("=== Running Basic Chart Tests ===")
    
    test_cases = get_test_cases()
    passed = 0
    failed = 0
    
    for test_name, chart_name, data, mappings in test_cases:
        if run_test(test_name, chart_name, data, mappings):
            passed += 1
        else:
            failed += 1
    
    print(f"\n=== Basic Test Summary ===")
    print(f"✅ Passed: {passed}")
    print(f"❌ Failed: {failed}")
    print(f"📊 Total: {passed + failed}")
    
    return {"passed": passed, "failed": failed, "total": passed + failed}

def run_performance_tests():
    """Run performance tests with larger datasets."""
    print(f"\n=== Performance Tests ===")
    
    large_data = {
        "x": list(range(1000)),
        "y": [i * 2 + (i % 10) for i in range(1000)],
        "category": [f"Cat_{i % 5}" for i in range(1000)]
    }
    
    start_time = time.time()
    
    try:
        html_path = create_temp_html_chart("scatter", json.dumps(large_data), {"x": "x", "y": "y", "color": "category"})
        end_time = time.time()
        print(f"✅ Large dataset (1000 points) rendered in {end_time - start_time:.2f} seconds")
        print(f"📁 File: {html_path}")
        return True
    except Exception as e:
        print(f"❌ Performance test failed: {e}")
        return False

def run_error_handling_tests():
    """Test error handling capabilities."""
    print(f"\n=== Error Handling Tests ===")
    
    test_cases = [
        ("Invalid Chart Type", "invalid_chart", {"x": [1, 2, 3], "y": [10, 15, 13]}, {"x": "x", "y": "y"}),
        ("Missing Column", "scatter", {"x": [1, 2, 3], "y": [10, 15, 13]}, {"x": "missing_column", "y": "y"}),
        ("Empty Dataset", "scatter", {}, {"x": "x", "y": "y"}),
        ("All NA Values", "scatter", {"x": ["na", "na", "na"], "y": ["na", "na", "na"]}, {"x": "x", "y": "y"}),
    ]
    
    passed = 0
    failed = 0
    
    for test_name, chart_name, data, mappings in test_cases:
        print(f"\n--- {test_name} ---")
        try:
            html_path = create_temp_html_chart(chart_name, json.dumps(data), mappings)
            print(f"✅ Error handling works: {html_path}")
            passed += 1
        except Exception as e:
            print(f"❌ Error handling test failed: {e}")
            failed += 1
    
    return {"passed": passed, "failed": failed, "total": passed + failed}

def run_chart_availability_tests():
    """Test chart availability and metadata."""
    print(f"\n=== Chart Availability Tests ===")
    
    available_charts = get_available_charts()
    print(f"Total available charts: {len(available_charts)}")
    
    # Group by type for better organisation
    try:
        import plotly.express as px
        px_charts = [attr for attr in dir(px) if not attr.startswith('_') and callable(getattr(px, attr))]
        
        print("\n📊 Plotly Express Charts:")
        for chart in sorted(px_charts):
            if chart in available_charts:
                print(f"  ✅ {chart}")
            else:
                print(f"  ❓ {chart} (not in available list)")
        
        print("\n🔧 Special Handler Charts:")
        special_charts = [chart for chart in available_charts if chart not in px_charts]
        for chart in sorted(special_charts):
            print(f"  ✅ {chart}")
        
        return True
    except Exception as e:
        print(f"❌ Chart availability test failed: {e}")
        return False

def run_data_validation_tests():
    """Test data validation and cleaning."""
    print(f"\n=== Data Validation Tests ===")
    
    test_datasets = [
        ("Clean Data", {"x": [1, 2, 3], "y": [10, 15, 13]}, {"x": "x", "y": "y"}),
        ("String Numbers", {"x": ["1", "2", "3"], "y": ["10", "15", "13"]}, {"x": "x", "y": "y"}),
        ("Mixed with NA", {"x": [1, "na", 3], "y": [10, 15, "N/A"]}, {"x": "x", "y": "y"}),
        ("Currency Format", {"x": ["$100", "$200", "$150"], "y": ["10%", "15%", "13%"]}, {"x": "x", "y": "y"}),
    ]
    
    passed = 0
    failed = 0
    
    for test_name, data, mappings in test_datasets:
        print(f"\n--- {test_name} ---")
        try:
            validation = validate_chart_mappings("scatter", mappings, data)
            if validation['errors']:
                print(f"  ❌ Validation errors: {validation['errors']}")
                failed += 1
            else:
                print(f"  ✅ Validation passed")
                if validation['warnings']:
                    print(f"  ⚠️  Warnings: {validation['warnings']}")
                passed += 1
        except Exception as e:
            print(f"  ❌ Validation failed: {e}")
            failed += 1
    
    return {"passed": passed, "failed": failed, "total": passed + failed}

def print_cleanup_info():
    """Print cleanup information for temporary files."""
    print(f"\n=== Cleanup Information ===")
    print("📝 Note: Temporary HTML files are created in the system temp directory.")
    print("🧹 To clean up: Check your system's temp folder and remove files matching '*_chart_name.html'")
    print("💡 On Unix systems: ls /tmp/*_*.html")
    print("💡 On Windows: dir %TEMP%\\*_*.html")

def main():
    """Run all tests."""
    try:
        print("=== Chart Renderer Test Suite ===")
        
        # Run all test suites
        basic_results = run_basic_tests()
        performance_ok = run_performance_tests()
        error_results = run_error_handling_tests()
        availability_ok = run_chart_availability_tests()
        validation_results = run_data_validation_tests()
        
        # Print overall summary
        print(f"\n=== Overall Test Summary ===")
        total_passed = basic_results["passed"] + error_results["passed"] + validation_results["passed"]
        total_failed = basic_results["failed"] + error_results["failed"] + validation_results["failed"]
        total_tests = total_passed + total_failed
        
        print(f"✅ Total Passed: {total_passed}")
        print(f"❌ Total Failed: {total_failed}")
        print(f"📊 Total Tests: {total_tests}")
        print(f"🚀 Performance Test: {'✅ PASSED' if performance_ok else '❌ FAILED'}")
        print(f"📋 Availability Test: {'✅ PASSED' if availability_ok else '❌ FAILED'}")
        
        if total_failed == 0 and performance_ok and availability_ok:
            print("\n🎉 All tests passed!")
        else:
            print(f"\n⚠️  {total_failed} tests failed. Check the logs above for details.")
        
        print_cleanup_info()
        
        return total_failed == 0
        
    except Exception as e:
        print(f"\n❌ Test suite failed with error: {e}")
        traceback.print_exc()
        return False

if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
