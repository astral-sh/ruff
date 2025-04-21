//! Heavily inspired by rustc data structures
use ruff_index::Idx;
use ruff_text_size::Ranged;
use std::fmt::{self, Display};

use crate::cfg::graph::{BlockId, BlockKind, Condition, ControlFlowGraph};

/// Returns control flow graph in Mermaid syntax.
pub fn draw_cfg<'src>(graph: ControlFlowGraph<'src>, source: &'src str) -> String {
    CFGWithSource::new(graph, source).draw_graph()
}

trait MermaidGraph: DirectedGraph {
    fn draw_node(&self, node: Self::Node) -> MermaidNode;
    fn draw_edges(&self, node: Self::Node) -> impl Iterator<Item = (Self::Node, MermaidEdge)>;

    fn draw_graph(&self) -> String {
        let mut graph = Vec::new();

        // Begin mermaid graph.
        graph.push("flowchart TD".to_string());

        // Draw nodes
        let num_nodes = self.num_nodes();
        for idx in 0..num_nodes {
            let node = Self::Node::new(idx);
            graph.push(format!("\tnode{}{}", idx, &self.draw_node(node)));
        }

        // Draw edges
        for idx in 0..num_nodes {
            graph.extend(
                self.draw_edges(Self::Node::new(idx))
                    .map(|(end_idx, edge)| format!("\tnode{}{}node{}", idx, edge, end_idx.index())),
            );
        }
        graph.join("\n")
    }
}

pub struct MermaidNode {
    shape: MermaidNodeShape,
    content: String,
}

impl MermaidNode {
    pub fn with_content(content: String) -> Self {
        Self {
            shape: MermaidNodeShape::default(),
            content,
        }
    }

    fn mermaid_write_quoted_str(f: &mut fmt::Formatter<'_>, value: &str) -> fmt::Result {
        let mut parts = value.split('"');
        if let Some(v) = parts.next() {
            write!(f, "{v}")?;
        }
        for v in parts {
            write!(f, "#quot;{v}")?;
        }
        Ok(())
    }
}

impl Display for MermaidNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (open, close) = self.shape.open_close();
        write!(f, "{open}\"")?;
        if self.content.is_empty() {
            write!(f, "empty")?;
        } else {
            MermaidNode::mermaid_write_quoted_str(f, &self.content)?;
        }
        write!(f, "\"{close}")
    }
}

#[derive(Debug, Default)]
pub enum MermaidNodeShape {
    #[default]
    Rectangle,
    DoubleRectangle,
    RoundedRectangle,
    Stadium,
    Circle,
    DoubleCircle,
    Asymmetric,
    Rhombus,
    Hexagon,
    Parallelogram,
    Trapezoid,
}

impl MermaidNodeShape {
    fn open_close(&self) -> (&'static str, &'static str) {
        match self {
            Self::Rectangle => ("[", "]"),
            Self::DoubleRectangle => ("[[", "]]"),
            Self::RoundedRectangle => ("(", ")"),
            Self::Stadium => ("([", "])"),
            Self::Circle => ("((", "))"),
            Self::DoubleCircle => ("(((", ")))"),
            Self::Asymmetric => (">", "]"),
            Self::Rhombus => ("{", "}"),
            Self::Hexagon => ("{{", "}}"),
            Self::Parallelogram => ("[/", "/]"),
            Self::Trapezoid => ("[/", "\\]"),
        }
    }
}

#[derive(Debug, Default)]
pub struct MermaidEdge {
    kind: MermaidEdgeKind,
    content: String,
}

impl Display for MermaidEdge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.content.is_empty() {
            write!(f, "{}", self.kind)
        } else {
            write!(f, "{}|\"{}\"|", self.kind, self.content)
        }
    }
}

#[derive(Debug, Default)]
pub enum MermaidEdgeKind {
    #[default]
    Arrow,
    DottedArrow,
    ThickArrow,
    BidirectionalArrow,
}

impl Display for MermaidEdgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MermaidEdgeKind::Arrow => write!(f, "-->"),
            MermaidEdgeKind::DottedArrow => write!(f, "-..->"),
            MermaidEdgeKind::ThickArrow => write!(f, "==>"),
            MermaidEdgeKind::BidirectionalArrow => write!(f, "<-->"),
        }
    }
}

pub trait DirectedGraph {
    type Node: Idx;

    fn num_nodes(&self) -> usize;
    fn start_node(&self) -> Self::Node;
    fn successors(&self, node: Self::Node) -> impl ExactSizeIterator<Item = Self::Node> + '_;
}

struct CFGWithSource<'source, 'stmt> {
    cfg: ControlFlowGraph<'stmt>,
    source: &'source str,
}

impl<'source, 'stmt> CFGWithSource<'source, 'stmt> {
    fn new(cfg: ControlFlowGraph<'stmt>, source: &'source str) -> Self {
        Self { cfg, source }
    }
}

impl DirectedGraph for CFGWithSource<'_, '_> {
    type Node = BlockId;

    fn num_nodes(&self) -> usize {
        self.cfg.num_blocks()
    }

    fn start_node(&self) -> Self::Node {
        self.cfg.initial()
    }

    fn successors(&self, node: Self::Node) -> impl ExactSizeIterator<Item = Self::Node> {
        self.cfg.outgoing(node).targets()
    }
}

impl MermaidGraph for CFGWithSource<'_, '_> {
    fn draw_node(&self, node: Self::Node) -> MermaidNode {
        let statements: Vec<String> = self
            .cfg
            .stmts(node)
            .iter()
            .map(|stmt| self.source[stmt.range()].to_string())
            .collect();
        let content = match self.cfg.kind(node) {
            BlockKind::Generic => {
                if statements.is_empty() {
                    "EMPTY".to_string()
                } else {
                    statements.join("\n")
                }
            }
            BlockKind::Start => {
                if statements.is_empty() {
                    "START".to_string()
                } else {
                    statements.join("\n")
                }
            }
            BlockKind::Terminal => {
                return MermaidNode {
                    content: "EXIT".to_string(),
                    shape: MermaidNodeShape::DoubleCircle,
                }
            }
            BlockKind::LoopGuard => {
                return MermaidNode {
                    content: "LOOP GUARD".to_string(),
                    shape: MermaidNodeShape::default(),
                }
            }
        };

        MermaidNode::with_content(content)
    }

    fn draw_edges(&self, node: Self::Node) -> impl Iterator<Item = (Self::Node, MermaidEdge)> {
        let edge_data = self.cfg.outgoing(node);
        edge_data
            .targets()
            .zip(edge_data.conditions())
            .map(|(target, condition)| {
                let edge = match condition {
                    Condition::Always => {
                        if target == self.cfg.terminal() {
                            MermaidEdge {
                                kind: MermaidEdgeKind::ThickArrow,
                                content: String::new(),
                            }
                        } else {
                            MermaidEdge {
                                kind: MermaidEdgeKind::Arrow,
                                content: String::new(),
                            }
                        }
                    }
                    Condition::Test(expr) => MermaidEdge {
                        kind: MermaidEdgeKind::Arrow,
                        content: self.source[expr.range()].to_string(),
                    },
                    Condition::Else => MermaidEdge {
                        kind: MermaidEdgeKind::Arrow,
                        content: "else".to_string(),
                    },
                    Condition::NotStopIter(expr) => MermaidEdge {
                        kind: MermaidEdgeKind::Arrow,
                        content: self.source[expr.range()].to_string(),
                    },
                    Condition::Match { subject: _, case } => MermaidEdge {
                        kind: MermaidEdgeKind::Arrow,
                        content: self.source[case.pattern.range()].to_string(),
                    },
                };
                (target, edge)
            })
            .collect::<Vec<_>>()
            .into_iter()
    }
}
