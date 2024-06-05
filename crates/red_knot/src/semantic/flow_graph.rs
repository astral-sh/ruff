use super::symbol_table::{Definition, SymbolId};
use crate::ast_ids::NodeKey;
use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast as ast;
use rustc_hash::FxHashMap;
use std::iter::FusedIterator;

#[newtype_index]
pub struct FlowNodeId;

#[derive(Debug)]
pub(crate) enum FlowNode {
    Start,
    Definition(DefinitionFlowNode),
    Branch(BranchFlowNode),
    Phi(PhiFlowNode),
}

/// A Definition node represents a point in control flow where a symbol is defined
#[derive(Debug)]
pub(crate) struct DefinitionFlowNode {
    symbol_id: SymbolId,
    definition: Definition,
    predecessor: FlowNodeId,
}

/// A Branch node represents a branch in control flow
#[derive(Debug)]
pub(crate) struct BranchFlowNode {
    predecessor: FlowNodeId,
}

/// A Phi node represents a join point where control flow paths come together
#[derive(Debug)]
pub(crate) struct PhiFlowNode {
    first_predecessor: FlowNodeId,
    second_predecessor: FlowNodeId,
}

#[derive(Debug)]
pub struct FlowGraph {
    flow_nodes_by_id: IndexVec<FlowNodeId, FlowNode>,
    ast_to_flow: FxHashMap<NodeKey, FlowNodeId>,
}

impl FlowGraph {
    pub fn start() -> FlowNodeId {
        FlowNodeId::from_usize(0)
    }

    pub fn for_expr(&self, expr: &ast::Expr) -> FlowNodeId {
        let node_key = NodeKey::from_node(expr.into());
        self.ast_to_flow[&node_key]
    }
}

#[derive(Debug)]
pub(crate) struct FlowGraphBuilder {
    flow_graph: FlowGraph,
}

impl FlowGraphBuilder {
    pub(crate) fn new() -> Self {
        let mut graph = FlowGraph {
            flow_nodes_by_id: IndexVec::default(),
            ast_to_flow: FxHashMap::default(),
        };
        graph.flow_nodes_by_id.push(FlowNode::Start);
        Self { flow_graph: graph }
    }

    pub(crate) fn add(&mut self, node: FlowNode) -> FlowNodeId {
        self.flow_graph.flow_nodes_by_id.push(node)
    }

    pub(crate) fn add_definition(
        &mut self,
        symbol_id: SymbolId,
        definition: Definition,
        predecessor: FlowNodeId,
    ) -> FlowNodeId {
        self.add(FlowNode::Definition(DefinitionFlowNode {
            symbol_id,
            definition,
            predecessor,
        }))
    }

    pub(crate) fn add_branch(&mut self, predecessor: FlowNodeId) -> FlowNodeId {
        self.add(FlowNode::Branch(BranchFlowNode { predecessor }))
    }

    pub(crate) fn add_phi(
        &mut self,
        first_predecessor: FlowNodeId,
        second_predecessor: FlowNodeId,
    ) -> FlowNodeId {
        self.add(FlowNode::Phi(PhiFlowNode {
            first_predecessor,
            second_predecessor,
        }))
    }

    pub(crate) fn record_expr(&mut self, expr: &ast::Expr, node_id: FlowNodeId) {
        self.flow_graph
            .ast_to_flow
            .insert(NodeKey::from_node(expr.into()), node_id);
    }

    pub(crate) fn finish(self) -> FlowGraph {
        self.flow_graph
    }
}

#[derive(Debug)]
pub struct ReachableDefinitionsIterator<'a> {
    flow_graph: &'a FlowGraph,
    symbol_id: SymbolId,
    pending: Vec<FlowNodeId>,
}

impl<'a> ReachableDefinitionsIterator<'a> {
    pub fn new(flow_graph: &'a FlowGraph, symbol_id: SymbolId, start_node_id: FlowNodeId) -> Self {
        Self {
            flow_graph,
            symbol_id,
            pending: vec![start_node_id],
        }
    }
}

impl<'a> Iterator for ReachableDefinitionsIterator<'a> {
    type Item = Definition;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let flow_node_id = self.pending.pop()?;
            match &self.flow_graph.flow_nodes_by_id[flow_node_id] {
                FlowNode::Start => return Some(Definition::None),
                FlowNode::Definition(def_node) => {
                    if def_node.symbol_id == self.symbol_id {
                        return Some(def_node.definition.clone());
                    }
                    self.pending.push(def_node.predecessor);
                }
                FlowNode::Branch(branch_node) => {
                    self.pending.push(branch_node.predecessor);
                }
                FlowNode::Phi(phi_node) => {
                    self.pending.push(phi_node.first_predecessor);
                    self.pending.push(phi_node.second_predecessor);
                }
            }
        }
    }
}

impl<'a> FusedIterator for ReachableDefinitionsIterator<'a> {}

impl std::fmt::Display for FlowGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "flowchart TD")?;
        for (id, node) in self.flow_nodes_by_id.iter_enumerated() {
            write!(f, "  id{}", id.as_u32())?;
            match node {
                FlowNode::Start => writeln!(f, r"[\Start/]")?,
                FlowNode::Definition(def_node) => {
                    writeln!(f, r"(Define symbol {})", def_node.symbol_id.as_u32())?;
                    writeln!(
                        f,
                        r"  id{}-->id{}",
                        def_node.predecessor.as_u32(),
                        id.as_u32()
                    )?;
                }
                FlowNode::Branch(branch_node) => {
                    writeln!(f, r"{{Branch}}")?;
                    writeln!(
                        f,
                        r"  id{}-->id{}",
                        branch_node.predecessor.as_u32(),
                        id.as_u32()
                    )?;
                }
                FlowNode::Phi(phi_node) => {
                    writeln!(f, r"((Phi))")?;
                    writeln!(
                        f,
                        r"  id{}-->id{}",
                        phi_node.second_predecessor.as_u32(),
                        id.as_u32()
                    )?;
                    writeln!(
                        f,
                        r"  id{}-->id{}",
                        phi_node.first_predecessor.as_u32(),
                        id.as_u32()
                    )?;
                }
            }
        }
        Ok(())
    }
}
