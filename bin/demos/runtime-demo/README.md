# Runtime Demo - SLEET Runtime Capabilities Showcase

This demo showcases the powerful runtime capabilities of the SLEET bytecode virtual machine and transpiler system, demonstrating features that set it apart from basic agent collaboration patterns.

## What This Demo Demonstrates

### 🔄 **Complete Transpilation Pipeline**

- **FlowDefinition → Contract → Bytecode** conversion
- Expression compilation with syntax validation
- State schema enforcement and type checking
- Source mapping for debugging and error reporting

### ⚡ **Dynamic Function Injection**

- Runtime registration of new FFI functions
- Versioned function management
- Hot-swappable computation logic
- External integration capabilities

### 🔥 **Hot Path Detection & JIT Compilation**

- Automatic profiling of execution patterns
- Hot path identification through execution counting
- JIT compilation triggers for performance optimisation
- Performance metrics collection and analysis

### 🔧 **Runtime Flow Modification**

- Live injection of new blocks into active flows
- Block replacement without restart
- Execution path modification during runtime
- Version-controlled flow evolution

### ⛽ **Gas Metering & Resource Management**

- Configurable gas budgets for execution control
- Resource consumption monitoring
- Execution limits and safety boundaries
- Cost-based optimisation decisions

### 🔌 **FFI Registry Integration**

- External function registration and management
- Type-safe function calling from bytecode
- State-aware function execution
- Dynamic capability extension

## Key Differences from Other Demos

Unlike `agent-orchestration-demo` (which focuses on agent collaboration) or `sleet-demo` (which shows basic workflow orchestration), this demo highlights SLEET's **unique runtime architecture**:

1. **Actual Transpilation**: Shows real FlowDefinition → Bytecode conversion
2. **Dynamic Injection**: Demonstrates runtime extensibility
3. **Performance Optimisation**: Hot path detection and JIT compilation
4. **Resource Management**: Gas metering and execution control
5. **Runtime Flexibility**: Live flow modification capabilities

## Usage

### Full Demo (Recommended)

```bash
cargo run --bin runtime-demo -- --mode full
```

### Individual Components

```bash
# Transpiler pipeline only
cargo run --bin runtime-demo -- --mode transpiler

# Dynamic function injection
cargo run --bin runtime-demo -- --mode dynamic

# Hot path optimisation
cargo run --bin runtime-demo -- --mode hotpath

# Runtime modification
cargo run --bin runtime-demo -- --mode modification
```

## Sample Output

The demo produces structured JSON logs showing:

```json
{
  "timestamp": "2025-08-02T10:30:45Z",
  "event": "transpiler_step_1",
  "data": {
    "step": "Flow Definition Created",
    "blocks": 6,
    "initial_state": { "counter": 0, "data": [1, 2, 3, 4, 5] },
    "has_schema": true
  }
}
```

## Technical Architecture

### Flow Definition

The demo creates a computational flow with:

- **State Management**: Counter, data arrays, thresholds
- **Conditional Logic**: Branching based on computed values
- **Expression Compilation**: Mathematical operations in bytecode
- **Resource Tracking**: Gas consumption monitoring

### Dynamic Functions

- `fibonacci`: Recursive computation for complexity testing
- `random_boost`: Randomised value enhancement
- `complex_calc`: State-aware multi-step calculation

### Runtime Modifications

- **Block Injection**: Insert performance boosters mid-execution
- **Block Replacement**: Swap logic without flow restart
- **Path Redirection**: Change execution flow dynamically

## Performance Insights

The demo reveals SLEET's performance characteristics:

1. **Transpilation Speed**: ~1-5ms for complex flows
2. **Execution Performance**: Sub-millisecond for simple operations
3. **Hot Path Detection**: 3+ executions trigger optimisation
4. **Gas Efficiency**: Varies by operation complexity (50-2000 gas)
5. **Memory Usage**: Minimal overhead for bytecode caching

## Real-World Applications

This architecture enables:

- **Live System Updates**: Modify running workflows without downtime
- **A/B Testing**: Swap execution paths for performance comparison
- **Resource Optimisation**: Gas-based cost control and limits
- **External Integration**: FFI functions for database, API, ML model calls
- **Performance Scaling**: JIT optimisation for high-throughput scenarios

## Comparison Matrix

| Feature              | agent-orchestration-demo | sleet-demo       | **runtime-demo**        |
| -------------------- | ------------------------ | ---------------- | ----------------------- |
| Agent Collaboration  | ✅ Advanced              | ✅ Basic         | ❌ Not Focus            |
| Transpiler Pipeline  | ❌ Simulated             | ❌ Hidden        | ✅ **Demonstrated**     |
| Dynamic Injection    | ❌ None                  | ❌ None          | ✅ **Core Feature**     |
| Hot Path Detection   | ❌ None                  | ❌ None          | ✅ **Live Profiling**   |
| Runtime Modification | ❌ None                  | ❌ None          | ✅ **Live Updates**     |
| Gas Metering         | ❌ None                  | ❌ None          | ✅ **Resource Control** |
| Performance Focus    | ❌ Collaboration         | ❌ Orchestration | ✅ **Runtime Power**    |

This demo showcases SLEET's **unique value proposition** as a high-performance, dynamically extensible execution runtime for agent-based systems.

Copyright (C) 2024 Jonathan Lee.
