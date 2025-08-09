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

import json
import pandas as pd
import plotly.express as px
import plotly.graph_objects as go
from typing import Dict, Any, Callable
import sys
import traceback
import os
import tempfile
import webbrowser

# --- Helper Functions ---

def _is_numeric_convertible(series: pd.Series, threshold: float = 0.8) -> bool:
    """Check if a series can be meaningfully converted to numeric."""
    if series.dtype in ['int64', 'float64', 'int32', 'float32']:
        return True
    
    if series.dtype == 'object':
        # Try conversion on a sample
        sample = series.dropna().head(100)
        if sample.empty:
            return False
            
        try:
            # Handle common missing value representations
            sample_clean = sample.replace(['na', 'Na', 'NA', 'n/a', 'N/A', ''], pd.NA)
            converted = pd.to_numeric(sample_clean, errors='coerce')
            success_rate = (converted.notna().sum() / len(sample_clean.dropna()))
            return success_rate >= threshold
        except:
            return False
    
    return False

def _clean_numeric_data(series: pd.Series) -> pd.Series:
    """Clean and convert series to numeric, handling missing values."""
    # Replace common missing value representations
    cleaned = series.replace(['na', 'Na', 'NA', 'n/a', 'N/A', '', 'null', 'None'], pd.NA)
    return pd.to_numeric(cleaned, errors='coerce')

# --- Handlers for Special Chart Types ---

def _handle_indicator(df: pd.DataFrame, mappings: Dict[str, str]) -> go.Figure:
    """Creates an indicator figure."""
    value_col = mappings.get('value')
    delta_col = mappings.get('delta_reference')
    
    if not value_col or value_col not in df.columns:
        raise ValueError("Indicator chart requires a 'value' mapping.")
    
    # Clean and convert the value
    value_data = _clean_numeric_data(df[value_col])
    if value_data.isna().all():
        raise ValueError("No valid numeric data found for indicator value.")
    
    value = value_data.dropna().iloc[0]
    
    delta_value = None
    if delta_col and delta_col in df.columns:
        delta_data = _clean_numeric_data(df[delta_col])
        if not delta_data.isna().all():
            delta_value = delta_data.dropna().iloc[0]

    return go.Figure(go.Indicator(
        mode="number+delta" if delta_value is not None else "number",
        value=value,
        delta={'reference': delta_value} if delta_value is not None else None,
        title={'text': mappings.get('title_text', value_col.title())}
    ))

def _handle_surface(df: pd.DataFrame, mappings: Dict[str, str]) -> go.Figure:
    """Creates a surface plot, pivoting data as needed."""
    # Debug: Print exactly what we received
    print(f"DEBUG: Surface handler received mappings: {mappings}", file=sys.stderr)
    print(f"DEBUG: DataFrame columns: {list(df.columns)}", file=sys.stderr)
    print(f"DEBUG: DataFrame shape: {df.shape}", file=sys.stderr)
    
    x_col, y_col, z_col = mappings.get('x'), mappings.get('y'), mappings.get('z')
    
    print(f"DEBUG: x_col={x_col}, y_col={y_col}, z_col={z_col}", file=sys.stderr)
    
    if not (x_col and y_col and z_col):
        missing = []
        if not x_col: missing.append('x')
        if not y_col: missing.append('y') 
        if not z_col: missing.append('z')
        raise ValueError(f"Surface chart requires 'x', 'y', and 'z' mappings. Missing: {missing}")

    # Check if columns exist
    for col_name, col_val in [('x', x_col), ('y', y_col), ('z', z_col)]:
        if col_val not in df.columns:
            raise ValueError(f"Column '{col_val}' for '{col_name}' not found in data. Available: {list(df.columns)}")

    try:
        # Clean data
        df_clean = df.copy()
        
        # Clean numeric data
        df_clean[x_col] = pd.to_numeric(df_clean[x_col], errors='coerce')
        df_clean[y_col] = pd.to_numeric(df_clean[y_col], errors='coerce')
        df_clean[z_col] = pd.to_numeric(df_clean[z_col], errors='coerce')
        
        # Remove rows with NaN values
        df_clean = df_clean.dropna(subset=[x_col, y_col, z_col])
        
        if df_clean.empty:
            raise ValueError("No valid data after removing NaN values")
        
        print(f"DEBUG: Clean data shape: {df_clean.shape}", file=sys.stderr)
        
        # Create pivot table
        try:
            pivot_df = df_clean.pivot_table(
                index=y_col, 
                columns=x_col, 
                values=z_col, 
                aggfunc='mean'  # Handle duplicates by averaging
            )
        except Exception as e:
            raise ValueError(f"Failed to pivot data for surface plot: {e}")
        
        print(f"DEBUG: Pivot table shape: {pivot_df.shape}", file=sys.stderr)
        
        # Create surface plot
        fig = go.Figure(data=[go.Surface(
            z=pivot_df.values,
            x=pivot_df.columns,
            y=pivot_df.index,
            colorscale='Viridis'
        )])
        
        fig.update_layout(
            scene=dict(
                xaxis_title=x_col,
                yaxis_title=y_col,
                zaxis_title=z_col,
                bgcolor="rgba(0,0,0,0)",
                camera=dict(eye=dict(x=1.5, y=1.5, z=1.5))
            ),
            title=f"Surface Plot: {z_col} vs {x_col} and {y_col}"
        )
        
        print("DEBUG: Surface plot created successfully", file=sys.stderr)
        return fig
        
    except Exception as e:
        print(f"DEBUG: Surface plot error: {e}", file=sys.stderr)
        raise ValueError(f"Failed to create surface plot: {e}")

def _handle_scatter_matrix(df: pd.DataFrame, mappings: Dict[str, str]) -> go.Figure:
    """Creates a scatter matrix, ensuring the 'dimensions' parameter is a list of numeric columns."""
    # Convert potential numeric columns
    df_clean = df.copy()
    for col in df_clean.columns:
        if _is_numeric_convertible(df_clean[col]):
            df_clean[col] = _clean_numeric_data(df_clean[col])
    
    numeric_cols = df_clean.select_dtypes(include=['number']).columns.tolist()
    if not numeric_cols:
        raise ValueError("Scatter matrix requires at least one numeric column in the data.")

    valid_mappings = mappings.copy()
    valid_mappings['dimensions'] = numeric_cols
    return px.scatter_matrix(df_clean, **valid_mappings)

def _handle_parallel_categories(df: pd.DataFrame, mappings: Dict[str, str]) -> go.Figure:
    """Creates a parallel categories plot, ensuring 'dimensions' is a list of categorical columns."""
    categorical_cols = df.select_dtypes(include=['object', 'category']).columns.tolist()
    if not categorical_cols:
        raise ValueError("Parallel categories chart requires at least one categorical (string) column.")

    valid_mappings = mappings.copy()
    valid_mappings['dimensions'] = categorical_cols
    
    if 'color' in valid_mappings:
        color_col = valid_mappings['color']
        if color_col in df.columns and not pd.api.types.is_numeric_dtype(df[color_col]):
            color_data = _clean_numeric_data(df[color_col])
            if not color_data.isna().all():
                df[color_col] = color_data
            else:
                del valid_mappings['color']
    
    return px.parallel_categories(df, **valid_mappings)

def _handle_parallel_coordinates(df: pd.DataFrame, mappings: Dict[str, str]) -> go.Figure:
    """Creates parallel coordinates plot with numeric dimensions."""
    # Convert potential numeric columns
    df_clean = df.copy()
    for col in df_clean.columns:
        if _is_numeric_convertible(df_clean[col]):
            df_clean[col] = _clean_numeric_data(df_clean[col])
    
    numeric_cols = df_clean.select_dtypes(include=['number']).columns.tolist()
    if not numeric_cols:
        raise ValueError("Parallel coordinates requires at least one numeric column.")
    
    valid_mappings = mappings.copy()
    valid_mappings['dimensions'] = numeric_cols
    return px.parallel_coordinates(df_clean, **valid_mappings)

def _handle_hierarchical_charts(df: pd.DataFrame, mappings: Dict[str, str], chart_type: str) -> go.Figure:
    """Handle treemap and sunburst charts with proper data validation."""
    values_col = mappings.get('values')
    names_col = mappings.get('names')
    
    if not values_col:
        raise ValueError(f"{chart_type} chart requires 'values' mapping.")
    
    # Clean the data
    df_clean = df.copy()
    
    # Handle missing values and convert to numeric
    if values_col in df_clean.columns:
        df_clean[values_col] = _clean_numeric_data(df_clean[values_col])
        df_clean = df_clean.dropna(subset=[values_col])
        
        # Ensure positive values for hierarchical charts
        df_clean = df_clean[df_clean[values_col] > 0]
    
    if df_clean.empty:
        raise ValueError(f"No valid positive numeric data found for {chart_type} chart.")
    
    # If no names provided, create indices or use row indices
    if not names_col or names_col not in df_clean.columns:
        df_clean['_auto_names'] = [f"Item {i+1}" for i in range(len(df_clean))]
        names_col = '_auto_names'
    
    # Aggregate data by names column (sum values for each unique name)
    agg_df = df_clean.groupby(names_col)[values_col].sum().reset_index()
    
    # Create the chart using Plotly Express
    if chart_type == 'treemap':
        return px.treemap(agg_df, 
                         path=[names_col],  # Use path instead of names
                         values=values_col,
                         title=f"Treemap: {values_col} by {names_col}")
    else:  # sunburst
        return px.sunburst(agg_df, 
                          path=[names_col],  # Use path instead of names
                          values=values_col,
                          title=f"Sunburst: {values_col} by {names_col}")

def _handle_treemap(df: pd.DataFrame, mappings: Dict[str, str]) -> go.Figure:
    """Creates a treemap chart with proper data handling."""
    return _handle_hierarchical_charts(df, mappings, 'treemap')

def _handle_sunburst(df: pd.DataFrame, mappings: Dict[str, str]) -> go.Figure:
    """Creates a sunburst chart with proper data handling."""
    return _handle_hierarchical_charts(df, mappings, 'sunburst')

def _handle_candlestick(df: pd.DataFrame, mappings: Dict[str, str]) -> go.Figure:
    """Creates a candlestick chart."""
    required = ['x', 'open', 'high', 'low', 'close']
    for col in required:
        if col not in mappings:
            raise ValueError(f"Candlestick chart requires '{col}' mapping.")
    
    # Clean numeric data
    df_clean = df.copy()
    for col in ['open', 'high', 'low', 'close']:
        if mappings[col] in df_clean.columns:
            df_clean[mappings[col]] = _clean_numeric_data(df_clean[mappings[col]])
    
    df_clean = df_clean.dropna(subset=[mappings[col] for col in required])
    if df_clean.empty:
        raise ValueError("No valid data for candlestick chart.")
    
    return go.Figure(data=[go.Candlestick(
        x=df_clean[mappings['x']],
        open=df_clean[mappings['open']],
        high=df_clean[mappings['high']],
        low=df_clean[mappings['low']],
        close=df_clean[mappings['close']]
    )])

def _handle_waterfall(df: pd.DataFrame, mappings: Dict[str, str]) -> go.Figure:
    """Creates a waterfall chart."""
    x_col = mappings.get('x')
    y_col = mappings.get('y')
    measure_col = mappings.get('measure')
    
    if not all([x_col, y_col]):
        raise ValueError("Waterfall chart requires 'x' and 'y' mappings.")
    
    # Clean the y data
    df_clean = df.copy()
    df_clean[y_col] = _clean_numeric_data(df_clean[y_col])
    df_clean = df_clean.dropna(subset=[y_col])
    
    if df_clean.empty:
        raise ValueError("No valid data for waterfall chart.")
    
    measure_values = df_clean[measure_col].tolist() if measure_col else ['relative'] * len(df_clean)
    
    return go.Figure(data=[go.Waterfall(
        x=df_clean[x_col],
        y=df_clean[y_col],
        measure=measure_values,
        name="Waterfall"
    )])

def _handle_sankey(df: pd.DataFrame, mappings: Dict[str, str]) -> go.Figure:
    """Creates a sankey diagram."""
    source_col = mappings.get('source')
    target_col = mappings.get('target')
    value_col = mappings.get('value')
    
    if not all([source_col, target_col, value_col]):
        raise ValueError("Sankey chart requires 'source', 'target', and 'value' mappings.")
    
    # Clean the data
    df_clean = df.copy()
    df_clean[value_col] = _clean_numeric_data(df_clean[value_col])
    df_clean = df_clean.dropna(subset=[source_col, target_col, value_col])
    df_clean = df_clean[df_clean[value_col] > 0]
    
    if df_clean.empty:
        raise ValueError("No valid data for sankey chart.")
    
    # Get unique labels
    labels = pd.concat([df_clean[source_col], df_clean[target_col]]).unique().tolist()
    label_map = {label: i for i, label in enumerate(labels)}
    
    return go.Figure(data=[go.Sankey(
        node=dict(label=labels),
        link=dict(
            source=[label_map[s] for s in df_clean[source_col]],
            target=[label_map[t] for t in df_clean[target_col]],
            value=df_clean[value_col]
        )
    )])

# --- Central Dispatcher for Special Handlers ---

SPECIAL_CHART_HANDLERS: Dict[str, Callable[[pd.DataFrame, Dict[str, str]], go.Figure]] = {
    'indicator': _handle_indicator,
    'surface': _handle_surface,
    'scatter_matrix': _handle_scatter_matrix,
    'parallel_categories': _handle_parallel_categories,
    'parallel_coordinates': _handle_parallel_coordinates,
    'candlestick': _handle_candlestick,
    'waterfall': _handle_waterfall,
    'sankey': _handle_sankey,
    'treemap': _handle_treemap,
    'sunburst': _handle_sunburst,
}

# --- Core Logic & Public API ---

def _create_error_figure(error_message: str) -> go.Figure:
    """Creates a visually clear error message figure."""
    fig = go.Figure()
    # Break the error message into lines for better readability
    wrapped_message = "<br>".join(error_message[i:i+80] for i in range(0, len(error_message), 80))
    fig.add_annotation(
        text=f"<b>Chart Rendering Error:</b><br>{wrapped_message}",
        xref="paper", yref="paper",
        x=0.5, y=0.5,
        showarrow=False,
        font=dict(size=14, color="red"),
        align="center",
        bordercolor="#c7c7c7",
        borderwidth=2,
        borderpad=4,
        bgcolor="#ff7f0e",
        opacity=0.8
    )
    fig.update_layout(
        xaxis=dict(visible=False),
        yaxis=dict(visible=False),
        plot_bgcolor="#f0f0f0"
    )
    return fig

def _get_base_layout(chart_name: str) -> go.Layout:
    """Returns a consistent base layout for all charts."""
    return go.Layout(
        title=dict(
            text=f"{chart_name.title()} Chart",
            font=dict(size=20),
            x=0.5,
            xanchor='center'
        ),
        template="plotly_white",
        font=dict(size=12),
        showlegend=True,
        margin=dict(l=50, r=50, t=80, b=50),
        hovermode='closest'
    )

def _create_figure(chart_name: str, data_json: str, mappings: Dict[str, str]) -> go.Figure:
    """Core private function to prepare data and create a Plotly Figure object."""
    print(f"DEBUG: _create_figure called with chart_name='{chart_name}'", file=sys.stderr)
    print(f"DEBUG: mappings={mappings}", file=sys.stderr)
    
    data = json.loads(data_json)
    df = pd.DataFrame(data)
    if df.empty:
        raise ValueError("Dataset is empty")
    
    print(f"DEBUG: DataFrame shape: {df.shape}", file=sys.stderr)
    print(f"DEBUG: DataFrame columns: {list(df.columns)}", file=sys.stderr)

    # Smart conversion based on chart requirements and data suitability
    for arg_name, column_name in mappings.items():
        if isinstance(column_name, str) and column_name in df.columns:
            col_data = df[column_name]
            
            # For arguments that should be numeric, try conversion if it makes sense
            if arg_name in ['x', 'y', 'z', 'size', 'values', 'value', 'r', 'theta', 'open', 'high', 'low', 'close'] and col_data.dtype == 'object':
                if _is_numeric_convertible(col_data):
                    print(f"DEBUG: Converting column '{column_name}' to numeric", file=sys.stderr)
                    df[column_name] = _clean_numeric_data(col_data)
            
            # Special handling for y-axis in bar charts - keep categorical data as strings
            elif arg_name == 'y' and chart_name == 'bar' and col_data.dtype == 'object':
                df[column_name] = col_data.astype(str)
            
            # For arguments that should be categorical, ensure they're strings
            elif arg_name in ['color', 'facet_row', 'facet_col', 'hover_name', 'symbol', 'line_dash', 'pattern_shape', 'names']:
                if col_data.dtype == 'object':
                    df[column_name] = col_data.astype(str)

    # Validate column existence
    for column in mappings.values():
        if isinstance(column, str) and column not in df.columns:
             raise ValueError(f"Column '{column}' not found. Available: {list(df.columns)}")

    print(f"DEBUG: Checking if '{chart_name}' is in SPECIAL_CHART_HANDLERS", file=sys.stderr)
    print(f"DEBUG: SPECIAL_CHART_HANDLERS keys: {list(SPECIAL_CHART_HANDLERS.keys())}", file=sys.stderr)
    
    if chart_name in SPECIAL_CHART_HANDLERS:
        print(f"DEBUG: Found special handler for '{chart_name}'", file=sys.stderr)
        handler = SPECIAL_CHART_HANDLERS[chart_name]
        print(f"DEBUG: About to call handler: {handler.__name__}", file=sys.stderr)
        return handler(df, mappings)
    else:
        print(f"DEBUG: Using standard plotly express for '{chart_name}'", file=sys.stderr)
        # Handle chart name aliases
        actual_chart_name = {
            'doughnut': 'pie', 
            'bubble': 'scatter'
        }.get(chart_name, chart_name)
        
        plot_function = getattr(px, actual_chart_name, None)

        if not callable(plot_function):
            available = [attr for attr in dir(px) if not attr.startswith('_') and callable(getattr(px, attr))]
            raise ValueError(f"Unknown or unsupported chart type '{chart_name}'. Available in px: {available}")
        
        fig = plot_function(df, **mappings)
        
        if chart_name == 'doughnut':
            fig.update_traces(hole=0.4)
        
        return fig

def render_chart(chart_name: str, data_json: str, mappings: Dict[str, str]) -> str:
    """Dynamically renders a chart, creating an error chart on failure."""
    try:
        fig = _create_figure(chart_name, data_json, mappings)
        fig.update_layout(_get_base_layout(chart_name))
        fig.update_layout(width=1000, height=600)
    except Exception as e:
        print(f"Error in render_chart: {str(e)}", file=sys.stderr)
        traceback.print_exc(file=sys.stderr)
        fig = _create_error_figure(str(e))
        fig.update_layout(width=1000, height=600)
    return fig.to_json()

def save_chart_as_html(chart_name: str, data_json: str, mappings: Dict[str, str], output_path: str) -> str:
    """Renders a chart to HTML, creating an error chart on failure."""
    try:
        fig = _create_figure(chart_name, data_json, mappings)
        fig.update_layout(_get_base_layout(chart_name))
        fig.update_layout(width=1200, height=700)
    except Exception as e:
        print(f"Error in save_chart_as_html: {str(e)}", file=sys.stderr)
        traceback.print_exc(file=sys.stderr)
        fig = _create_error_figure(str(e))
        fig.update_layout(width=1200, height=700)
    
    config = {'displayModeBar': True, 'displaylogo': False, 'modeBarButtonsToRemove': ['select2d', 'lasso2d']}
    fig.write_html(output_path, config=config, include_plotlyjs='cdn')
    return output_path

def create_temp_html_chart(chart_name: str, data_json: str, mappings: Dict[str, str]) -> str:
    """Creates a temporary HTML file for the chart."""
    fd, temp_path = tempfile.mkstemp(suffix=f"_{chart_name}.html")
    os.close(fd)
    save_chart_as_html(chart_name, data_json, mappings, temp_path)
    return temp_path

def open_chart_in_browser(chart_name: str, data_json: str, mappings: Dict[str, str]) -> str:
    """Creates a chart HTML file and opens it in the default browser."""
    html_path = create_temp_html_chart(chart_name, data_json, mappings)
    webbrowser.open(f'file://{os.path.abspath(html_path)}')
    return html_path

def get_available_charts() -> list:
    """Returns a list of all available chart types."""
    px_charts = [attr for attr in dir(px) if not attr.startswith('_') and callable(getattr(px, attr))]
    special_charts = list(SPECIAL_CHART_HANDLERS.keys())
    aliases = ['doughnut', 'bubble']
    return sorted(set(px_charts + special_charts + aliases))

def validate_chart_mappings(chart_name: str, mappings: Dict[str, str], data: Dict) -> Dict[str, Any]:
    """Validates that the chart mappings are valid for the given data."""
    df = pd.DataFrame(data)
    errors = []
    warnings = []
    
    # Check if columns exist
    for arg, col in mappings.items():
        if isinstance(col, str) and col not in df.columns:
            errors.append(f"Column '{col}' not found in data")
    
    # Check for data quality issues
    for arg, col in mappings.items():
        if isinstance(col, str) and col in df.columns:
            col_data = df[col]
            
            # Check for missing values
            null_pct = col_data.isnull().sum() / len(col_data) * 100
            if null_pct > 50:
                warnings.append(f"Column '{col}' has {null_pct:.1f}% missing values")
            
            # Check for "na" string values
            if col_data.dtype == 'object':
                na_count = col_data.isin(['na', 'Na', 'NA', 'n/a', 'N/A', '', 'null', 'None']).sum()
                if na_count > 0:
                    warnings.append(f"Column '{col}' has {na_count} 'na' string values that will be cleaned")
    
    # Check specific chart requirements
    if chart_name == 'surface':
        required = ['x', 'y', 'z']
        for req in required:
            if req not in mappings:
                errors.append(f"Surface chart requires '{req}' mapping")
    
    elif chart_name == 'candlestick':
        required = ['x', 'open', 'high', 'low', 'close']
        for req in required:
            if req not in mappings:
                errors.append(f"Candlestick chart requires '{req}' mapping")
    
    elif chart_name == 'sankey':
        required = ['source', 'target', 'value']
        for req in required:
            if req not in mappings:
                errors.append(f"Sankey chart requires '{req}' mapping")
    
    elif chart_name == 'indicator':
        if 'value' not in mappings:
            errors.append("Indicator chart requires 'value' mapping")
    
    elif chart_name in ['treemap', 'sunburst']:
        if 'values' not in mappings:
            errors.append(f"{chart_name} chart requires 'values' mapping")
        if 'names' not in mappings:
            warnings.append(f"{chart_name} chart works better with 'names' mapping (will auto-generate if missing)")
    
    elif chart_name in ['scatter_matrix', 'parallel_coordinates']:
        # Check for numeric columns
        numeric_cols = []
        for col in df.columns:
            if _is_numeric_convertible(df[col]):
                numeric_cols.append(col)
        if not numeric_cols:
            errors.append(f"{chart_name} requires at least one numeric column")
    
    elif chart_name == 'parallel_categories':
        categorical_cols = df.select_dtypes(include=['object', 'category']).columns.tolist()
        if not categorical_cols:
            errors.append("Parallel categories requires at least one categorical column")
    
    return {
        'valid': len(errors) == 0, 
        'errors': errors,
        'warnings': warnings
    }

def create_sample_data(chart_type: str) -> Dict[str, Any]:
    """Create sample data for testing different chart types."""
    if chart_type == 'surface':
        return {
            'x': [1, 1, 2, 2, 3, 3, 4, 4] * 3,
            'y': [1, 2, 1, 2, 1, 2, 1, 2] * 3,
            'z': [10, 15, 12, 18, 14, 20, 16, 22] * 3
        }
    elif chart_type == 'candlestick':
        return {
            'date': ['2023-01-01', '2023-01-02', '2023-01-03', '2023-01-04', '2023-01-05'],
            'open': [100, 102, 101, 103, 105],
            'high': [105, 107, 106, 108, 110],
            'low': [99, 100, 99, 101, 103],
            'close': [102, 101, 103, 105, 107]
        }
    elif chart_type == 'sankey':
        return {
            'source': ['A', 'A', 'B', 'B', 'C'],
            'target': ['B', 'C', 'C', 'D', 'D'],
            'value': [10, 5, 8, 3, 7]
        }
    elif chart_type in ['treemap', 'sunburst']:
        return {
            'names': ['A', 'B', 'C', 'D', 'E'],
            'values': [10, 15, 12, 8, 20]
        }
    elif chart_type == 'indicator':
        return {
            'metric': [85],
            'target': [100]
        }
    else:
        # Default sample data
        return {
            'x': [1, 2, 3, 4, 5],
            'y': [10, 15, 13, 17, 20],
            'category': ['A', 'B', 'A', 'B', 'A']
        }

# Entry point for testing (removed comprehensive test suite)
if __name__ == "__main__":
    print("Chart Renderer Module")
    print("For comprehensive testing, run: python test_renderer.py")
    print(f"Available charts: {len(get_available_charts())}")
    
    # Simple smoke test
    try:
        sample_data = create_sample_data('scatter')
        result = render_chart('scatter', json.dumps(sample_data), {'x': 'x', 'y': 'y'})
        print("✅ Basic functionality test passed")
    except Exception as e:
        print(f"❌ Basic functionality test failed: {e}")
