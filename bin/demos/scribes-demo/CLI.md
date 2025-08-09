# Scribes Demo CLI Usage

The enhanced STELE Scribes Demo now supports command-line arguments for configurable execution.

## Command Line Options

### Basic Usage

```bash
cargo run --bin scribes-demo [OPTIONS]
```

### Available Options

#### `--trace`

Enable trace-level logging for maximum verbosity. This provides detailed internal logs from all components including SurrealDB transactions, HTTP requests, and low-level operations.

```bash
cargo run --bin scribes-demo -- --trace
```

#### `--stress-tests`

Run additional stress testing scenarios. This includes:

- High-volume data processing tests
- Edge case scenario handling
- Concurrent operations testing
- Failure recovery scenarios

```bash
cargo run --bin scribes-demo -- --stress-tests
```

#### `--log-level <LEVEL>`

Set the logging level explicitly. Available levels: `error`, `warn`, `info`, `debug`, `trace`
Default: `debug`

```bash
cargo run --bin scribes-demo -- --log-level warn
cargo run --bin scribes-demo -- --log-level trace
```

#### `--help`

Display help information

```bash
cargo run --bin scribes-demo -- --help
```

#### `--version`

Display version information

```bash
cargo run --bin scribes-demo -- --version
```

## Usage Examples

### Default Execution (Debug Logging, No Stress Tests)

```bash
cargo run --bin scribes-demo
```

### Maximum Verbosity with Stress Tests

```bash
cargo run --bin scribes-demo -- --trace --stress-tests
```

### Quiet Execution with Stress Tests

```bash
cargo run --bin scribes-demo -- --log-level warn --stress-tests
```

### Trace Logging Only

```bash
cargo run --bin scribes-demo -- --trace
```

## Logging Behaviour

- **Default**: Debug-level logging to both stdout and rotating log files
- **Trace Flag**: When `--trace` is used, enables maximum verbosity
- **Log Level**: When `--log-level` is specified, it overrides the trace flag
- **File Logging**: All executions create rotating daily log files in `logs/scribes-demo.log.*`
- **LLM Logging**: Comprehensive LLM interaction logs are saved to `logs/llm_interactions.jsonl`

## Stress Testing

When `--stress-tests` is enabled, the demo will run additional comprehensive tests:

1. **High Volume Processing**: Tests system performance under load
2. **Edge Case Scenarios**: Tests handling of unusual inputs and edge cases
3. **Concurrent Operations**: Tests multi-threaded processing capabilities
4. **Failure Recovery**: Tests graceful handling of error conditions

These tests provide detailed metrics on:

- Success rates
- Processing times
- Error handling quality
- System resilience

## Default Behaviour

Without any flags:

- Uses debug-level logging
- Skips stress tests (with informational message)
- Runs core functionality demonstrations
- Creates comprehensive logs for troubleshooting

This provides a quick demonstration while allowing users to opt into more intensive testing and verbose logging as needed.

Copyright (C) 2024 Jonathan Lee.
