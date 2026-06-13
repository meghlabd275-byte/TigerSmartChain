/**
 * Advanced Contract Visualization - Call Graph, Inheritance Graph, Storage Layout Viewer
 * Complete implementation in Rust for high performance and ultra-low latency
 * 
 * This module provides:
 * - Call graph analysis and visualization
 * - Inheritance graph analysis
 * - Storage layout analysis
 * - Bytecode analysis
 * - Security vulnerability detection (reentrancy, unprotected functions)
 */

use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};

// ============================================
// Core Types for Contract Analysis
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub address: String,
    pub name: String,
    pub source_code: String,
    pub abi: Vec<Function>,
    pub bytecode: Vec<u8>,
    pub storage: Vec<StorageVariable>,
    pub functions: Vec<Function>,
    pub events: Vec<Event>,
    pub inheritance: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub visibility: Visibility,
    pub state_mutability: StateMutability,
    pub inputs: Vec<Parameter>,
    pub outputs: Vec<Parameter>,
    pub modifiers: Vec<String>,
    pub body: Option<Vec<Statement>>,
    pub called_functions: Vec<String>,
    pub calling_contracts: Vec<String>,
    pub raw_function_selector: Option<[u8; 4]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Visibility {
    Public,
    Private,
    Internal,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StateMutability {
    Pure,
    View,
    Nonpayable,
    Payable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub param_type: Type,
    pub storage_slot: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Type {
    pub base: String,
    pub array_length: Option<usize>,
    pub mapping: Option<(Box<Type>, Box<Type>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Statement {
    Assignment { left: Expression, right: Expression },
    FunctionCall { function: String, arguments: Vec<Expression> },
    Conditional { condition: Expression, then: Vec<Statement>, else_: Vec<Statement> },
    Return(Expression),
    Emit { event: String, arguments: Vec<Expression> },
    StorageUpdate { slot: u32, value: Expression },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expression {
    pub kind: ExpressionKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExpressionKind {
    Variable(String),
    Literal(Literal),
    BinaryOp { left: Box<Expression>, op: BinaryOp, right: Box<Expression> },
    UnaryOp { op: UnaryOp, expr: Box<Expression> },
    MemberAccess { object: Box<Expression>, member: String },
    IndexAccess { array: Box<Expression>, index: Box<Expression> },
    FunctionCall { function: Box<Expression>, arguments: Vec<Expression> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod, Exp,
    Equal, NotEqual, LessThan, GreaterThan, LessThanOrEqual, GreaterThanOrEqual,
    And, Or,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UnaryOp {
    Not, Neg, BitNot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Literal {
    Bool(bool),
    Int(u128),
    Address(String),
    String(String),
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub name: String,
    pub inputs: Vec<Parameter>,
    pub anonymous: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageVariable {
    pub name: String,
    pub param_type: Type,
    pub slot: u32,
    pub offset: u32,
    pub value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageLayout {
    pub slot: u32,
    pub offset: u32,
    pub contract: String,
    pub variable: String,
    pub type_name: String,
    pub bytes: u32,
}

// ============================================
// Call Graph Analysis
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallGraph {
    pub nodes: Vec<CallNode>,
    pub edges: Vec<CallEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallNode {
    pub id: String,
    pub label: String,
    pub function: String,
    pub contract: String,
    pub node_type: NodeType,
    pub visibility: Visibility,
    pub entry_points: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    Function,
    Modifier,
    Constructor,
    Fallback,
    Receive,
    Library,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEdge {
    pub from: String,
    pub to: String,
    pub edge_type: EdgeType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EdgeType {
    DirectCall,
    DelegateCall,
    StaticCall,
    ExternalCall,
    InternalCall,
    LibraryCall,
}

// ============================================
// Inheritance Graph
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InheritanceGraph {
    pub contracts: Vec<InheritanceNode>,
    pub edges: Vec<InheritanceEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InheritanceNode {
    pub id: String,
    pub name: String,
    pub contract_type: ContractType,
    pub linearization: Vec<String>,
    pub functions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContractType {
    Contract,
    Library,
    Interface,
    Abstract,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InheritanceEdge {
    pub from: String,
    pub to: String,
    pub edge_type: InheritanceType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InheritanceType {
    Is,
    Library,
}

// ============================================
// Contract Analyzer
// ============================================

pub struct ContractAnalyzer {
    contracts: HashMap<String, Contract>,
    call_graph: CallGraph,
    inheritance_graph: InheritanceGraph,
    storage_layouts: HashMap<String, Vec<StorageLayout>>,
}

impl ContractAnalyzer {
    pub fn new() -> Self {
        Self {
            contracts: HashMap::new(),
            call_graph: CallGraph { nodes: Vec::new(), edges: Vec::new() },
            inheritance_graph: InheritanceGraph { contracts: Vec::new(), edges: Vec::new() },
            storage_layouts: HashMap::new(),
        }
    }

    pub fn add_contract(&mut self, contract: Contract) {
        let address = contract.address.clone();
        self.contracts.insert(address, contract);
    }

    pub fn analyze(&mut self) -> Result<(), AnalysisError> {
        self.build_call_graph()?;
        self.build_inheritance_graph()?;
        self.analyze_storage_layouts()?;
        Ok(())
    }

    fn build_call_graph(&mut self) -> Result<(), AnalysisError> {
        for (address, contract) in &self.contracts {
            for function in &contract.functions {
                let node_id = format!("{}.{}", address, function.name);
                
                let node = CallNode {
                    id: node_id.clone(),
                    label: function.name.clone(),
                    function: function.name.clone(),
                    contract: address.clone(),
                    node_type: NodeType::Function,
                    visibility: function.visibility.clone(),
                    entry_points: Vec::new(),
                };
                
                self.call_graph.nodes.push(node);
                
                for called in &function.called_functions {
                    let edge = CallEdge {
                        from: node_id.clone(),
                        to: called.clone(),
                        edge_type: EdgeType::DirectCall,
                    };
                    self.call_graph.edges.push(edge);
                }
                
                for ext_contract in &function.calling_contracts {
                    let edge = CallEdge {
                        from: node_id.clone(),
                        to: format!("{}.{}", ext_contract, function.name),
                        edge_type: EdgeType::ExternalCall,
                    };
                    self.call_graph.edges.push(edge);
                }
            }
        }
        Ok(())
    }

    fn build_inheritance_graph(&mut self) -> Result<(), AnalysisError> {
        for (address, contract) in &self.contracts {
            let node = InheritanceNode {
                id: address.clone(),
                name: contract.name.clone(),
                contract_type: ContractType::Contract,
                linearization: contract.inheritance.clone(),
                functions: contract.functions.iter().map(|f| f.name.clone()).collect(),
            };
            
            self.inheritance_graph.contracts.push(node);
            
            for parent in &contract.inheritance {
                let edge = InheritanceEdge {
                    from: address.clone(),
                    to: parent.clone(),
                    edge_type: InheritanceType::Is,
                };
                self.inheritance_graph.edges.push(edge);
            }
        }
        Ok(())
    }

    fn analyze_storage_layouts(&mut self) -> Result<(), AnalysisError> {
        for (address, contract) in &self.contracts {
            let mut layouts: Vec<StorageLayout> = Vec::new();
            let mut current_slot: u32 = 0;
            let mut current_offset: u32 = 0;
            
            for var in &contract.storage {
                let bytes = var.param_type.get_bytes();
                
                if current_offset + bytes > 32 {
                    current_slot += 1;
                    current_offset = 0;
                }
                
                layouts.push(StorageLayout {
                    slot: current_slot,
                    offset: current_offset,
                    contract: address.clone(),
                    variable: var.name.clone(),
                    type_name: format!("{:?}", var.param_type),
                    bytes,
                });
                
                current_offset += bytes;
                if current_offset >= 32 {
                    current_slot += 1;
                    current_offset = 0;
                }
            }
            
            self.storage_layouts.insert(address.clone(), layouts);
        }
        Ok(())
    }

    pub fn get_call_graph(&self) -> &CallGraph {
        &self.call_graph
    }

    pub fn get_inheritance_graph(&self) -> &InheritanceGraph {
        &self.inheritance_graph
    }

    pub fn get_storage_layout(&self, address: &str) -> Option<&Vec<StorageLayout>> {
        self.storage_layouts.get(address)
    }

    pub fn find_callers(&self, target: &str) -> Vec<String> {
        self.call_graph.edges
            .iter()
            .filter(|e| e.to == target)
            .map(|e| e.from.clone())
            .collect()
    }

    pub fn find_reachable(&self, start: &str) -> Vec<String> {
        let mut reachable = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = vec![start.to_string()];
        
        while let Some(current) = queue.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());
            reachable.push(current.clone());
            
            for edge in &self.call_graph.edges {
                if edge.from == current && !visited.contains(&edge.to) {
                    queue.push(edge.to.clone());
                }
            }
        }
        
        reachable
    }

    pub fn detect_reentrancy(&self) -> Vec<ReentrancyWarning> {
        let mut warnings = Vec::new();
        
        for node in &self.call_graph.nodes {
            for edge in &self.call_graph.edges {
                if edge.from == node.id {
                    if edge.edge_type == EdgeType::ExternalCall || edge.edge_type == EdgeType::DelegateCall {
                        warnings.push(ReentrancyWarning {
                            function: node.function.clone(),
                            contract: node.contract.clone(),
                            severity: Severity::High,
                            description: "Potential reentrancy: external call after function entry".to_string(),
                        });
                    }
                }
            }
        }
        
        warnings
    }

    pub fn detect_unprotected(&self) -> Vec<UnprotectedWarning> {
        let mut warnings = Vec::new();
        
        for node in &self.call_graph.nodes {
            if node.visibility == Visibility::Public {
                if let Some(contract) = self.contracts.get(&node.contract) {
                    if let Some(function) = contract.functions.iter().find(|f| f.name == node.function) {
                        if function.modifiers.is_empty() {
                            warnings.push(UnprotectedWarning {
                                function: node.function.clone(),
                                contract: node.contract.clone(),
                                severity: Severity::Medium,
                                description: "Unprotected public function - consider adding access control".to_string(),
                            });
                        }
                    }
                }
            }
        }
        
        warnings
    }
}

// ============================================
// Type Implementation
// ============================================

impl Type {
    pub fn get_bytes(&self) -> u32 {
        match self.base.as_str() {
            "bool" => 1,
            "uint8" | "int8" => 1,
            "uint16" | "int16" => 2,
            "uint32" | "int32" | "address" => 4,
            "uint64" | "int64" => 8,
            "uint128" | "int128" => 16,
            "uint256" | "int256" | "bytes32" => 32,
            "bytes" | "string" => 32,
            _ => 32,
        }
    }
}

// ============================================
// Storage Layout Analysis
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageAnalysis {
    pub contract: String,
    pub layout: Vec<StorageVariable>,
    pub packed_slots: Vec<PackedSlot>,
    pub gaps: Vec<StorageGap>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackedSlot {
    pub slot: u32,
    pub variables: Vec<PackedVariable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackedVariable {
    pub name: String,
    pub offset: u32,
    pub bytes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageGap {
    pub slot: u32,
    pub start_offset: u32,
    pub end_offset: u32,
    pub size: u32,
}

impl StorageAnalysis {
    pub fn analyze(contract: &Contract) -> Self {
        let mut layout: Vec<StorageVariable> = Vec::new();
        let mut current_slot: u32 = 0;
        let mut current_offset: u32 = 0;
        
        for var in &contract.storage {
            let bytes = var.param_type.get_bytes();
            
            if current_offset + bytes > 32 {
                current_slot += 1;
                current_offset = 0;
            }
            
            let mut var_with_layout = var.clone();
            var_with_layout.slot = current_slot;
            var_with_layout.offset = current_offset;
            
            layout.push(var_with_layout);
            
            current_offset += bytes;
            if current_offset >= 32 {
                current_slot += 1;
                current_offset = 0;
            }
        }
        
        let mut packed_slots: Vec<PackedSlot> = Vec::new();
        let mut current_vars: Vec<PackedVariable> = Vec::new();
        let mut last_slot: u32 = 0;
        
        for var in &layout {
            if var.slot != last_slot {
                if !current_vars.is_empty() {
                    packed_slots.push(PackedSlot {
                        slot: last_slot,
                        variables: current_vars.clone(),
                    });
                }
                current_vars = Vec::new();
                last_slot = var.slot;
            }
            
            current_vars.push(PackedVariable {
                name: var.name.clone(),
                offset: var.offset,
                bytes: var.param_type.get_bytes(),
            });
        }
        
        if !current_vars.is_empty() {
            packed_slots.push(PackedSlot {
                slot: last_slot,
                variables: current_vars,
            });
        }
        
        let mut gaps: Vec<StorageGap> = Vec::new();
        for slot_data in &packed_slots {
            let mut last_end: u32 = 0;
            for var in &slot_data.variables {
                if var.offset > last_end {
                    gaps.push(StorageGap {
                        slot: slot_data.slot,
                        start_offset: last_end,
                        end_offset: var.offset,
                        size: var.offset - last_end,
                    });
                }
                last_end = var.offset + var.bytes;
            }
            
            if last_end < 32 {
                gaps.push(StorageGap {
                    slot: slot_data.slot,
                    start_offset: last_end,
                    end_offset: 32,
                    size: 32 - last_end,
                });
            }
        }
        
        StorageAnalysis {
            contract: contract.name.clone(),
            layout,
            packed_slots,
            gaps,
        }
    }
}

// ============================================
// Bytecode Analysis
// ============================================

pub struct BytecodeAnalyzer {
    pub instructions: Vec<Instruction>,
    pub jumps: Vec<JumpTarget>,
    pub storage_ops: Vec<StorageOp>,
    pub external_calls: Vec<ExternalCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instruction {
    pub offset: usize,
    pub opcode: String,
    pub args: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JumpTarget {
    pub from: usize,
    pub to: usize,
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageOp {
    pub offset: usize,
    pub op_type: StorageOpType,
    pub slot: Option<Expression>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageOpType {
    Sload,
    Sstore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalCall {
    pub offset: usize,
    pub target: Expression,
    pub value: Option<Expression>,
    pub data: Option<Expression>,
}

impl BytecodeAnalyzer {
    pub fn new(bytecode: &[u8]) -> Self {
        let mut analyzer = BytecodeAnalyzer {
            instructions: Vec::new(),
            jumps: Vec::new(),
            storage_ops: Vec::new(),
            external_calls: Vec::new(),
        };
        
        analyzer.analyze(bytecode);
        analyzer
    }

    fn analyze(&mut self, bytecode: &[u8]) {
        let mut offset = 0;
        
        while offset < bytecode.len() {
            let opcode = bytecode[offset];
            
            match opcode {
                0x60..=0x7f => {
                    let num_bytes = (opcode - 0x60 + 1) as usize;
                    let mut args = Vec::new();
                    for i in 1..=num_bytes {
                        if offset + i < bytecode.len() {
                            args.push(bytecode[offset + i]);
                        }
                    }
                    
                    self.instructions.push(Instruction {
                        offset,
                        opcode: format!("PUSH{}", num_bytes),
                        args: if args.is_empty() { None } else { Some(args) },
                    });
                    
                    offset += num_bytes + 1;
                }
                0x56 => {
                    self.instructions.push(Instruction {
                        offset,
                        opcode: "JUMP".to_string(),
                        args: None,
                    });
                    offset += 1;
                }
                0x57 => {
                    self.instructions.push(Instruction {
                        offset,
                        opcode: "JUMPI".to_string(),
                        args: None,
                    });
                    offset += 1;
                }
                0x54 => {
                    self.storage_ops.push(StorageOp {
                        offset,
                        op_type: StorageOpType::Sload,
                        slot: None,
                    });
                    self.instructions.push(Instruction {
                        offset,
                        opcode: "SLOAD".to_string(),
                        args: None,
                    });
                    offset += 1;
                }
                0x55 => {
                    self.storage_ops.push(StorageOp {
                        offset,
                        op_type: StorageOpType::Sstore,
                        slot: None,
                    });
                    self.instructions.push(Instruction {
                        offset,
                        opcode: "SSTORE".to_string(),
                        args: None,
                    });
                    offset += 1;
                }
                0xf1 => {
                    self.instructions.push(Instruction {
                        offset,
                        opcode: "CALL".to_string(),
                        args: None,
                    });
                    offset += 1;
                }
                0xf2 => {
                    self.instructions.push(Instruction {
                        offset,
                        opcode: "STATICCALL".to_string(),
                        args: None,
                    });
                    offset += 1;
                }
                0xf3 => {
                    self.instructions.push(Instruction {
                        offset,
                        opcode: "DELEGATECALL".to_string(),
                        args: None,
                    });
                    offset += 1;
                }
                _ => {
                    self.instructions.push(Instruction {
                        offset,
                        opcode: format!("{:02x}", opcode),
                        args: None,
                    });
                    offset += 1;
                }
            }
        }
    }

    pub fn get_jump_targets(&self) -> Vec<JumpTarget> {
        self.jumps.clone()
    }

    pub fn get_storage_ops(&self) -> Vec<StorageOp> {
        self.storage_ops.clone()
    }

    pub fn get_external_calls(&self) -> Vec<ExternalCall> {
        self.external_calls.clone()
    }
}

// ============================================
// Security Warnings
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReentrancyWarning {
    pub function: String,
    pub contract: String,
    pub severity: Severity,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnprotectedWarning {
    pub function: String,
    pub contract: String,
    pub severity: Severity,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnalysisError {
    ParseError(String),
    TypeError(String),
    IOError(String),
}

// ============================================
// Main Analysis Function
// ============================================

pub fn analyze_contract(contract: Contract) -> Result<ContractAnalysis, AnalysisError> {
    let mut analyzer = ContractAnalyzer::new();
    analyzer.add_contract(contract);
    
    analyzer.analyze()?;
    
    let contracts = analyzer.contracts;
    let contract = contracts.values().next().unwrap();
    
    Ok(ContractAnalysis {
        contract: contract.name.clone(),
        call_graph: analyzer.get_call_graph().clone(),
        inheritance_graph: analyzer.get_inheritance_graph().clone(),
        storage_layouts: analyzer.get_storage_layout(&contract.address).cloned(),
        reentrancy_warnings: analyzer.detect_reentrancy(),
        unprotected_warnings: analyzer.detect_unprotected(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractAnalysis {
    pub contract: String,
    pub call_graph: CallGraph,
    pub inheritance_graph: InheritanceGraph,
    pub storage_layouts: Option<Vec<StorageLayout>>,
    pub reentrancy_warnings: Vec<ReentrancyWarning>,
    pub unprotected_warnings: Vec<UnprotectedWarning>,
}

// ============================================
// Export Functions
// ============================================

pub fn export_to_json(analysis: &ContractAnalysis) -> Result<String, AnalysisError> {
    serde_json::to_string_pretty(analysis)
        .map_err(|e| AnalysisError::IOError(e.to_string()))
}

pub fn export_to_dot(analysis: &ContractAnalysis) -> String {
    let mut dot = String::new();
    
    dot.push_str("digraph contract {\n");
    dot.push_str("  rankdir=LR;\n");
    dot.push_str("  node [shape=box];\n\n");
    
    for node in &analysis.call_graph.nodes {
        dot.push_str(&format!("  \"{}\" [label=\"{}\"];\n", node.id, node.label));
    }
    
    dot.push('\n');
    
    for edge in &analysis.call_graph.edges {
        dot.push_str(&format!(
            "  \"{}\" -> \"{}\" [label=\"{:?}\"];\n",
            edge.from, edge.to, edge.edge_type
        ));
    }
    
    dot.push_str("}\n");
    
    dot
}

// ============================================
// Tests
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_bytes() {
        let t = Type {
            base: "uint256".to_string(),
            array_length: None,
            mapping: None,
        };
        
        assert_eq!(t.get_bytes(), 32);
    }

    #[test]
    fn test_storage_analysis() {
        let contract = Contract {
            address: "0x1234".to_string(),
            name: "Test".to_string(),
            source_code: String::new(),
            abi: Vec::new(),
            bytecode: Vec::new(),
            storage: vec![
                StorageVariable {
                    name: "owner".to_string(),
                    param_type: Type {
                        base: "address".to_string(),
                        array_length: None,
                        mapping: None,
                    },
                    slot: 0,
                    offset: 0,
                    value: None,
                },
            ],
            functions: Vec::new(),
            events: Vec::new(),
            inheritance: Vec::new(),
        };
        
        let analysis = StorageAnalysis::analyze(&contract);
        assert!(!analysis.layout.is_empty());
    }

    #[test]
    fn test_bytecode_analyzer() {
        let bytecode = vec![0x60, 0x01, 0x60, 0x02, 0x01];
        let analyzer = BytecodeAnalyzer::new(&bytecode);
        assert!(!analyzer.instructions.is_empty());
    }
}