use super::symbol_table::SymbolId;
use crate::semantic::{Definition, ExpressionId};
use ruff_index::{newtype_index, IndexVec};
use std::iter::FusedIterator;

#[newtype_index]
pub struct FlowNodeId;

#[derive(Debug)]
pub(crate) enum FlowNode {
    Start,
    Definition(DefinitionFlowNode),
    Branch(BranchFlowNode),
    Phi(PhiFlowNode),
    Constraint(ConstraintFlowNode),
}

/// A point in control flow where a symbol is defined
#[derive(Debug)]
pub(crate) struct DefinitionFlowNode {
    symbol_id: SymbolId,
    definition: Definition,
    predecessor: FlowNodeId,
}

/// A branch in control flow
#[derive(Debug)]
pub(crate) struct BranchFlowNode {
    predecessor: FlowNodeId,
}

/// A join point where control flow paths come together
#[derive(Debug)]
pub(crate) struct PhiFlowNode {
    first_predecessor: FlowNodeId,
    second_predecessor: FlowNodeId,
}

/// A branch test which may apply constraints to a symbol's type
#[derive(Debug)]
pub(crate) struct ConstraintFlowNode {
    predecessor: FlowNodeId,
    test_expression: ExpressionId,
}

#[derive(Debug)]
pub struct FlowGraph {
    flow_nodes_by_id: IndexVec<FlowNodeId, FlowNode>,
    expression_map: IndexVec<ExpressionId, FlowNodeId>,
}

impl FlowGraph {
    pub fn start() -> FlowNodeId {
        FlowNodeId::from_usize(0)
    }

    pub fn for_expr(&self, expr: ExpressionId) -> FlowNodeId {
        self.expression_map[expr]
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
            expression_map: IndexVec::default(),
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

    pub(crate) fn add_constraint(
        &mut self,
        predecessor: FlowNodeId,
        test_expression: ExpressionId,
    ) -> FlowNodeId {
        self.add(FlowNode::Constraint(ConstraintFlowNode {
            predecessor,
            test_expression,
        }))
    }

    pub(super) fn record_expr(&mut self, node_id: FlowNodeId) -> ExpressionId {
        self.flow_graph.expression_map.push(node_id)
    }

    pub(super) fn finish(mut self) -> FlowGraph {
        self.flow_graph.flow_nodes_by_id.shrink_to_fit();
        self.flow_graph.expression_map.shrink_to_fit();
        self.flow_graph
    }
}

#[derive(Debug, Clone)]
pub struct ConstrainedDefinition {
    pub definition: Definition,
    pub constraints: Vec<ExpressionId>,
}

#[derive(Debug)]
pub struct ReachableDefinitionsIterator<'a> {
    flow_graph: &'a FlowGraph,
    symbol_id: SymbolId,
    pending: Vec<FlowNodeId>,
    constraints: Vec<ExpressionId>,
}

impl<'a> ReachableDefinitionsIterator<'a> {
    pub fn new(flow_graph: &'a FlowGraph, symbol_id: SymbolId, start_node_id: FlowNodeId) -> Self {
        Self {
            flow_graph,
            symbol_id,
            pending: vec![start_node_id],
            constraints: vec![],
        }
    }
}

impl<'a> Iterator for ReachableDefinitionsIterator<'a> {
    type Item = ConstrainedDefinition;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let flow_node_id = self.pending.pop()?;
            match &self.flow_graph.flow_nodes_by_id[flow_node_id] {
                FlowNode::Start => {
                    // constraints on unbound are irrelevant
                    self.constraints.clear();
                    return Some(ConstrainedDefinition {
                        definition: Definition::Unbound,
                        constraints: vec![],
                    });
                }
                FlowNode::Definition(def_node) => {
                    if def_node.symbol_id == self.symbol_id {
                        let mut constrained_def = ConstrainedDefinition {
                            definition: def_node.definition.clone(),
                            constraints: vec![],
                        };
                        // TODO this isn't correct, some constraints will still apply to the
                        // next pending flow node; need to record constraints with each pending
                        // node
                        std::mem::swap(&mut constrained_def.constraints, &mut self.constraints);
                        return Some(constrained_def);
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
                FlowNode::Constraint(constraint_node) => {
                    self.pending.push(constraint_node.predecessor);
                    self.constraints.push(constraint_node.test_expression);
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
                FlowNode::Constraint(constraint_node) => {
                    writeln!(f, r"((Constraint))")?;
                    writeln!(
                        f,
                        r"  id{}-->id{}",
                        constraint_node.predecessor.as_u32(),
                        id.as_u32()
                    )?;
                }
            }
        }
        Ok(())
    }
}
