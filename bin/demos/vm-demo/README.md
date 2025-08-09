# VM Demo: Bytecode Virtual Machine Implementation

## Overview

This demonstration showcases a basic stack-based bytecode virtual machine implemented in Rust. The VM provides fundamental instruction execution capabilities with a straightforward interpreter design.

## Externalized configuration

- FlowDefinition (orchestration): `bin/demos/vm-demo/flows/simple_flow.yaml`
  - YAML enum tags are used for block variants (e.g., `!Compute`, `!AwaitInput`, `!ForEach`).
- VM contracts (bytecode tests and benchmark):
  - `bin/demos/vm-demo/contracts/comprehensive_contract.json`
  - `bin/demos/vm-demo/contracts/hybrid_benchmark_contract.json`
  - Generated on first run from the built-in defaults, then loaded from disk. You may replace with JSON or YAML; the loader picks by file extension. Delete to regenerate from defaults.

## Running the Demo

From the workspace root:

```bash
cargo run -p vm-demo
```

Or from this folder:

```bash
cd bin/demos/vm-demo
cargo run
```

Expected output (abridged):

```
Enhanced BytecodeVM Demo - Comprehensive OpCode Showcase

 Executing multi-block contract with hybrid JIT/FFI flow...
 JIT compiler initialized successfully
 Multi-block demo completed successfully.
 Final status: AwaitingInput {...}

 Running hybrid micro-benchmark (compute-only loop with single FFI print)...
 JIT compiler initialized successfully
 Hybrid micro-benchmark completed: AwaitingInput {...}
 Timing: total=... ms, FFI=... ms, approx compute=... ms

 Running transpiled FlowDefinition via FlowTranspiler -> VM...
 JIT compiler initialized successfully
 Transpiled flow run status: AwaitingInput { interaction_id: "review_plan", agent_id: "reviewer", ... }

 Running orchestration flow via OrchestrationCoordinator...
 Orchestrated flow status: AwaitingInput { interaction_id: "generate_plan", agent_id: "planner", ... }
```

Notes:

- The two VM phases end at an AwaitingInput placeholder by design.
- The flow/transpiler and coordinator phases use the IDs from `simple_flow.yaml`.

## Customize flows and contracts

- Edit the orchestration flow at `flows/simple_flow.yaml`.
  - Ensure block variants use YAML tags: `!Compute`, `!Conditional`, `!AwaitInput`, `!ForEach`, `!Continue`, `!Break`, `!Terminate`.
  - Common YAML error: `invalid type: map, expected a YAML tag starting with '!'` → add the correct tag.
- Edit or replace the VM contracts at `contracts/*.json|*.yaml`.
  - Remove the files to regenerate from built-in defaults on next run.

## What This Demo Actually Does

### Core Functionality

- **Bytecode Execution**: Implements 29 basic opcodes including arithmetic, comparison, logical operations, and control flow
- **Stack-Based Architecture**: Uses a traditional stack-based execution model for operand management
- **Foreign Function Interface**: Supports external function calls through a registry system
- **Gas Metering**: Provides basic resource consumption tracking to prevent infinite loops
- **Error Handling**: Includes comprehensive error reporting for runtime issues

### Instruction Set

The VM supports the following categories of operations:

**Arithmetic Operations:**

- Add, Subtract, Multiply, Divide, Modulo, Negate

**Comparison Operations:**

- Equal, NotEqual, GreaterThan, LessThan, GreaterEqual, LessEqual

**Logical Operations:**

- And, Or, Not

**Stack Operations:**

- Push, Pop, Dup (duplicate), Swap

**Control Flow:**

- Jump, JumpIfFalse, JumpIfTrue, Call, Return, Halt

**Variable Operations:**

- LoadVar, StoreVar, LoadIndex

**External Integration:**

- CallFfi (Foreign Function Interface)

## Test Suite Structure

The demo executes six sequential test phases:

1. **Arithmetic Test**: Demonstrates basic mathematical operations (15 + 8 = 23, comparison with 20)
2. **Control Flow Test**: Shows conditional branching using jump instructions
3. **Function Test**: Exercises mathematical operations and stack manipulation
4. **Loop Test**: Implements simple iteration patterns
5. **FFI Test**: Calls external functions including Fibonacci calculation and assertion checking
6. **New OpCodes Test**: Validates recently implemented comparison and logical operations

Additionally:

- A compute-only hybrid micro-benchmark runs with timing breakdown (total vs. FFI vs. approx compute).
- The same flow runs via the VM (FlowTranspiler path) and via the OrchestrationCoordinator.

## Performance Characteristics

This is a **straightforward interpreter implementation** with the following characteristics:

- Execution speed typical of tree-walking or bytecode interpreters
- Linear execution without optimisation
- Stack operations with standard overhead
- FFI calls with function pointer dispatch

## Technical Implementation

### Architecture

- `RemarkableInterpreter`: Main execution coordinator
- `BytecodeVM`: Core instruction processor with stack management
- `FfiRegistry`: External function call dispatcher
- `Contract`: Bytecode container with execution metadata

### Execution Flow

1. Bytecode is hashed and recorded in profiler (for tracking only)
2. A new VM instance is created for each execution
3. Instructions are processed sequentially with gas consumption tracking
4. FFI calls are dispatched through the registry when encountered
5. Execution continues until halt instruction or error condition

## Dependencies

This demo uses workspace-managed dependencies:

- `sleet` (workspace path crate)
- `tokio` (workspace, full features)
- `serde`, `serde_json`, `serde_yaml` (workspace)

<details>
<summary>Click to expand the sequence diagram</summary>

::: mermaid

---

config:
theme: neutral

---

sequenceDiagram
participant User
participant Main
participant RemarkableInterpreter
participant Contract
participant BytecodeVM
participant FFIRegistry
participant Profiler as Profiler (Stub)
participant JITCache as JIT Cache (Empty)

    User->>Main: cargo run
    Main->>Main: Setup FFI registry with print, fibonacci, assert_equal

    Note over Main: Create comprehensive bytecode with 6 test sections
    Main->>Main: build_arithmetic_test() -> Vec<u8>
    Main->>Main: build_new_opcodes_test() -> Vec<u8>

    Main->>Contract: Create single-block contract with bytecode
    Main->>RemarkableInterpreter: new(gas=1M, contract, ffi_registry)
    RemarkableInterpreter->>JITCache: new() (empty HashMap)
    RemarkableInterpreter->>Profiler: new() (empty hot_paths HashMap)

    Main->>RemarkableInterpreter: run(contract)

    loop Execute Contract Blocks
        RemarkableInterpreter->>RemarkableInterpreter: execute_node(block)
        RemarkableInterpreter->>RemarkableInterpreter: run_vm(bytecode)

        Note over RemarkableInterpreter: Hash bytecode & record in profiler
        RemarkableInterpreter->>Profiler: record(bytecode_hash)
        RemarkableInterpreter->>JITCache: get(bytecode_hash) -> None (always empty)

        Note over RemarkableInterpreter: Create BytecodeVM instance
        RemarkableInterpreter->>BytecodeVM: new(bytecode, state, ffi_registry, permissions)
        RemarkableInterpreter->>BytecodeVM: run(&mut gas)

        loop Execute Bytecode Instructions
            BytecodeVM->>BytecodeVM: read_opcode()

            alt Arithmetic Operations
                Note over BytecodeVM: Push 15, Push 8, Add -> 23
                BytecodeVM->>BytecodeVM: OpCode::Push (15)
                BytecodeVM->>BytecodeVM: OpCode::Push (8)
                BytecodeVM->>BytecodeVM: OpCode::Add -> Stack: [23]

                Note over BytecodeVM: Push 20, GreaterThan -> true
                BytecodeVM->>BytecodeVM: OpCode::Push (20)
                BytecodeVM->>BytecodeVM: OpCode::GreaterThan -> Stack: [true]

                Note over BytecodeVM: Push false, NotEqual -> true
                BytecodeVM->>BytecodeVM: OpCode::Push (false)
                BytecodeVM->>BytecodeVM: OpCode::NotEqual -> Stack: [true]

            else New OpCodes Test
                Note over BytecodeVM: 15 >= 15 = true, 10 <= 15 = true
                BytecodeVM->>BytecodeVM: OpCode::Push (15)
                BytecodeVM->>BytecodeVM: OpCode::Push (15)
                BytecodeVM->>BytecodeVM: OpCode::GreaterEqual -> Stack: [true]
                BytecodeVM->>BytecodeVM: OpCode::Push (10)
                BytecodeVM->>BytecodeVM: OpCode::Push (15)
                BytecodeVM->>BytecodeVM: OpCode::LessEqual -> Stack: [true, true]
                BytecodeVM->>BytecodeVM: OpCode::And -> Stack: [true]

            else FFI Calls
                Note over BytecodeVM: Call fibonacci(8)
                BytecodeVM->>BytecodeVM: OpCode::Push (8)
                BytecodeVM->>BytecodeVM: OpCode::CallFfi ("fibonacci", 1)
                BytecodeVM->>FFIRegistry: call fibonacci([8])
                FFIRegistry->>FFIRegistry: fibonacci(8) -> 21
                FFIRegistry-->>BytecodeVM: 21
                BytecodeVM->>BytecodeVM: Stack: [21]

                Note over BytecodeVM: Assert 21 == 21
                BytecodeVM->>BytecodeVM: OpCode::Push (21)
                BytecodeVM->>BytecodeVM: OpCode::CallFfi ("assert_equal", 2)
                BytecodeVM->>FFIRegistry: call assert_equal([21, 21])
                FFIRegistry->>FFIRegistry: print("✓ Assertion passed: 21 == 21")
                FFIRegistry-->>BytecodeVM: true

            else Print Operations
                BytecodeVM->>BytecodeVM: OpCode::Push ("message")
                BytecodeVM->>BytecodeVM: OpCode::CallFfi ("print", 1)
                BytecodeVM->>FFIRegistry: call print(["message"])
                FFIRegistry->>User: Output: message
                FFIRegistry-->>BytecodeVM: null

            else Control Flow
                BytecodeVM->>BytecodeVM: OpCode::JumpIfFalse, OpCode::Jump
                Note over BytecodeVM: Conditional branching works correctly

            else Stack Operations
                BytecodeVM->>BytecodeVM: OpCode::Dup, OpCode::Swap, OpCode::Pop
                Note over BytecodeVM: Stack manipulation works correctly
            end
        end

        BytecodeVM-->>RemarkableInterpreter: execution result

        Note over RemarkableInterpreter: Check if path is hot (>100 executions)
        RemarkableInterpreter->>Profiler: is_hot(bytecode_hash) -> false (never hot in demo)
        alt Path is Hot (Never Happens)
            RemarkableInterpreter->>RemarkableInterpreter: print("[JIT] Path is hot! Triggering compilation")
            Note over RemarkableInterpreter: ⚠️ NO ACTUAL COMPILATION OCCURS
        end

        RemarkableInterpreter-->>Main: ExecutionStatus::Running
    end

    RemarkableInterpreter-->>Main: ExecutionStatus::Completed(result)
    Main->>User: "✓ Enhanced demo completed successfully!"

:::

</details>

Copyright (C) 2024 Jonathan Lee.
