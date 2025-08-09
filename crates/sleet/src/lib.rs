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

pub mod agents;
pub mod ast;
pub mod flows;
pub mod llm;
pub mod logging;
pub mod orchestration;
pub mod runtime;
pub mod tasks;
pub mod transpiler;
pub mod workflows;
pub use agents::{
    Agent, AgentCapabilities, AgentError, AgentSystem, AgentSystemConfig, CapabilityMatcher,
    CapabilityMatcherConfig, FallbackAgentConfig, GenerationConfig,
};
pub use ast::{AstNode, Contract, Literal, Op, Path, PathSegment, SourceLocation};
pub use flows::definition::{BlockDefinition, BlockType, FlowDefinition};
pub use llm::{LLMError, LLMProcessor, UnifiedLLMAdapter};
pub use orchestration::{
    EventSystem, ExecutionContext, OrchestrationConfig, OrchestrationCoordinator,
    OrchestrationError, OrchestrationFlowDefinition, OrchestrationResult, OrchestrationSession,
    ResourceManager,
};
use runtime::{ExecutionStatus, FfiRegistry, RemarkableInterpreter};
use serde_json::Value;
use std::collections::HashMap;
pub use stele::LLMConfig;
pub use tasks::{
    Task, TaskConfig, TaskError, TaskExecution, TaskProposal, TaskSystem, TaskSystemConfig,
};
pub use transpiler::{FlowTranspiler, TranspilerError};
pub use workflows::{
    events, generate_complete_team, PlanningSession, PlanningSessionConfig, TeamGenerationConfig,
};
pub async fn execute_flow(
    flow_def: FlowDefinition,
    initial_gas: u64,
    ffi_registry: Option<FfiRegistry>,
) -> Result<ExecutionStatus, Box<dyn std::error::Error>> {
    let orchestration_contract = FlowTranspiler::transpile(&flow_def)?;
    let contract = convert_contract(orchestration_contract)?;
    let mut runtime =
        RemarkableInterpreter::new(initial_gas, &contract, ffi_registry.unwrap_or_default())?;
    let result = runtime.run(contract).await?;
    Ok(result)
}

pub async fn execute_orchestrated_flow(
    flow_def: OrchestrationFlowDefinition,
    gas_limit: Option<u64>,
    config: Option<OrchestrationConfig>,
) -> OrchestrationResult<ExecutionStatus> {
    let config = config.unwrap_or_default();
    let coordinator = OrchestrationCoordinator::new(config).await?;
    coordinator.execute_flow(flow_def, gas_limit).await
}

pub async fn create_orchestration_coordinator(
    config: OrchestrationConfig,
    agent_system: Option<AgentSystem>,
    llm_processor: Option<LLMProcessor>,
    task_system: Option<TaskSystem>,
) -> OrchestrationResult<OrchestrationCoordinator> {
    let mut coordinator = OrchestrationCoordinator::new(config).await?;
    coordinator
        .initialise(agent_system, llm_processor, task_system)
        .await?;
    Ok(coordinator)
}
pub fn convert_contract(
    orchestration_contract: transpiler::orchestration::ast::Contract,
) -> Result<Contract, Box<dyn std::error::Error>> {
    let converted_blocks: Result<HashMap<String, AstNode>, _> = orchestration_contract
        .blocks
        .into_iter()
        .map(|(id, node)| convert_ast_node(node).map(|converted_node| (id, converted_node)))
        .collect();
    let converted_blocks = converted_blocks?;
    let converted_initial_state = convert_ast_node(orchestration_contract.initial_state)?;

    Ok(Contract {
        version: orchestration_contract.version,
        start_block_id: orchestration_contract.start_block_id,
        blocks: converted_blocks,
        initial_state: converted_initial_state,
        permissions: serde_json::to_value(orchestration_contract.permissions)?,
        participants: orchestration_contract.participants,
    })
}
pub fn convert_ast_node(
    orchestration_node: transpiler::orchestration::ast::AstNode,
) -> Result<AstNode, Box<dyn std::error::Error>> {
    let converted_op = convert_op(orchestration_node.op)?;
    Ok(AstNode {
        op: converted_op,
        metadata: orchestration_node.metadata,
        source_location: None,
    })
}
pub fn convert_op(
    orchestration_op: transpiler::orchestration::ast::Op,
) -> Result<Op, Box<dyn std::error::Error>> {
    use transpiler::orchestration::ast::Op as OrchOp;
    match orchestration_op {
        OrchOp::Literal(lit) => Ok(Op::Literal(convert_literal(lit)?)),
        OrchOp::Fetch(path_segments) => {
            let converted_path = convert_path_segments(path_segments);

            Ok(Op::Evaluate {
                bytecode: vec![],
                output_path: converted_path,
            })
        }
        OrchOp::Assign { path, value } => {
            let converted_path = convert_path_segments(path);

            Ok(Op::Sequence(vec![
                convert_ast_node(*value)?,
                AstNode {
                    op: Op::Evaluate {
                        bytecode: vec![],
                        output_path: converted_path,
                    },
                    metadata: HashMap::new(),
                    source_location: None,
                },
            ]))
        }
        OrchOp::Sequence(nodes) => {
            let converted_nodes: Result<Vec<AstNode>, _> =
                nodes.into_iter().map(convert_ast_node).collect();
            Ok(Op::Sequence(converted_nodes?))
        }
        OrchOp::If {
            condition,
            then_branch,
            else_branch,
        } => Ok(Op::If {
            condition: Box::new(convert_ast_node(*condition)?),
            then_branch: Box::new(convert_ast_node(*then_branch)?),
            else_branch: match else_branch {
                Some(eb) => Some(Box::new(convert_ast_node(*eb)?)),
                None => None,
            },
        }),
        OrchOp::Evaluate {
            bytecode,
            output_path,
        } => {
            let converted_path = convert_path_segments(output_path);
            Ok(Op::Evaluate {
                bytecode,
                output_path: converted_path,
            })
        }
        OrchOp::Await {
            interaction_id,
            agent_id,
            prompt,
            timeout_ms,
        } => Ok(Op::Await {
            interaction_id,
            agent_id,
            prompt: match prompt {
                Some(p) => Some(Box::new(convert_ast_node(*p)?)),
                None => Some(Box::new(AstNode {
                    op: Op::Literal(Literal::String("".to_string())),
                    metadata: HashMap::new(),
                    source_location: None,
                })),
            },
            timeout_ms,
        }),
        OrchOp::SetNextBlock(block_id) => Ok(Op::SetNextBlock(block_id)),
        OrchOp::Terminate => Ok(Op::Terminate),
        
        OrchOp::PushErrorHandler { catch_block_id } => Ok(Op::PushErrorHandler { catch_block_id }),
        OrchOp::PopErrorHandler => Ok(Op::PopErrorHandler),
        OrchOp::Length(node) => Ok(Op::Length(Box::new(convert_ast_node(*node)?))),
        OrchOp::Add(left, right) => Ok(Op::Add(
            Box::new(convert_ast_node(*left)?),
            Box::new(convert_ast_node(*right)?),
        )),
        OrchOp::LessThan(left, right) => Ok(Op::LessThan(
            Box::new(convert_ast_node(*left)?),
            Box::new(convert_ast_node(*right)?),
        )),
    }
}

fn convert_path_segments(path_segments: Vec<transpiler::orchestration::ast::PathSegment>) -> Path {
    let segments = path_segments
        .into_iter()
        .map(|segment| match segment {
            transpiler::orchestration::ast::PathSegment::State => PathSegment::State,
            transpiler::orchestration::ast::PathSegment::Input => PathSegment::Input,
            transpiler::orchestration::ast::PathSegment::Key(key) => PathSegment::Key(key),
            transpiler::orchestration::ast::PathSegment::Index(idx) => PathSegment::Index(idx),
            transpiler::orchestration::ast::PathSegment::DynamicOffset(node) => {
                PathSegment::DynamicOffset(Box::new(
                    convert_ast_node(*node)
                        .unwrap_or_else(|_| AstNode::from(Op::Literal(Literal::Number(0.0)))),
                ))
            }
        })
        .collect();
    Path(segments)
}

fn convert_literal(
    lit: transpiler::orchestration::ast::Literal,
) -> Result<Literal, Box<dyn std::error::Error>> {
    use transpiler::orchestration::ast::Literal as OrchLiteral;
    match lit {
        OrchLiteral::Null => Ok(Literal::Null),
        OrchLiteral::Bool(b) => Ok(Literal::Bool(b)),
        OrchLiteral::Number(n) => Ok(Literal::Number(n)),
        OrchLiteral::String(s) => Ok(Literal::String(s)),
        OrchLiteral::Array(arr) => {
            let nodes: Result<Vec<AstNode>, _> = arr.into_iter().map(convert_ast_node).collect();
            Ok(Literal::Array(nodes?))
        }
        OrchLiteral::Object(obj) => {
            let map: Result<HashMap<String, AstNode>, _> = obj
                .into_iter()
                .map(|(k, node)| convert_ast_node(node).map(|v| (k, v)))
                .collect();
            Ok(Literal::Object(map?))
        }
    }
}

pub fn convert_literal_to_value(
    lit: transpiler::orchestration::ast::Literal,
) -> Result<Value, Box<dyn std::error::Error>> {
    let unified_literal = convert_literal(lit)?;
    Ok(unified_literal.into())
}
