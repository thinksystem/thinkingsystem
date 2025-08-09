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
Convenience script to run all renderer tests.
"""

import sys
import os

# Add current directory to path for imports
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

def main():
    print("=== Running Chart Renderer Tests ===")
    
    # Test basic import
    try:
        from renderer import render_chart, get_available_charts
        print("✅ Successfully imported renderer module")
    except Exception as e:
        print(f"❌ Failed to import renderer: {e}")
        return False
    
    # Run comprehensive test suite
    try:
        from test_renderer import main as run_tests
        success = run_tests()
        return success
    except Exception as e:
        print(f"❌ Failed to run tests: {e}")
        return False

if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
