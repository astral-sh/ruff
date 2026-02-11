use std::cmp::Ordering;
use std::collections::BTreeMap;

use ruff_db::files::FilePath;

use crate::node_key::NodeKey;
use crate::semantic_index::ast_ids::ScopedUseId;
use crate::semantic_index::predicate::ScopedPredicateId;
use crate::semantic_index::reachability_constraints::ScopedReachabilityConstraintId;
use crate::semantic_index::{FileScopeId, ScopedEnclosingSnapshotId};

/// TDD root that we want to measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum TddRootKind {
    NodeReachability,
    NarrowingConstraint,
}

impl TddRootKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::NodeReachability => "reachability",
            Self::NarrowingConstraint => "narrowing",
        }
    }
}

/// Source location for a measured root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum TddRootSource {
    Node(NodeKey),
    Use {
        use_id: ScopedUseId,
        binding: u32,
    },
    EnclosingSnapshot {
        snapshot_id: ScopedEnclosingSnapshotId,
        binding: Option<u32>,
    },
}

/// Debug reference to a measured root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TddRootRef {
    pub(crate) kind: TddRootKind,
    pub(crate) source: TddRootSource,
    pub(crate) constraint: ScopedReachabilityConstraintId,
}

/// Size stats for one root constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TddRootStat {
    pub(crate) root: TddRootRef,
    pub(crate) interior_nodes: usize,
}

/// Hot interior node aggregated across multiple roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TddHotNodeStat {
    pub(crate) kind: TddRootKind,
    pub(crate) constraint: ScopedReachabilityConstraintId,
    pub(crate) predicate: ScopedPredicateId,
    pub(crate) subtree_interior_nodes: usize,
    pub(crate) root_uses: usize,
    pub(crate) score: usize,
    pub(crate) sample_roots: Vec<TddRootRef>,
}

impl Ord for TddRootRef {
    fn cmp(&self, other: &Self) -> Ordering {
        self.kind
            .cmp(&other.kind)
            .then_with(|| self.constraint.as_u32().cmp(&other.constraint.as_u32()))
            .then_with(|| self.source.cmp(&other.source))
    }
}

impl PartialOrd for TddRootRef {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TddRootStat {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .interior_nodes
            .cmp(&self.interior_nodes)
            .then_with(|| self.root.cmp(&other.root))
    }
}

impl PartialOrd for TddRootStat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TddHotNodeStat {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .cmp(&self.score)
            .then_with(|| {
                other
                    .subtree_interior_nodes
                    .cmp(&self.subtree_interior_nodes)
            })
            .then_with(|| other.root_uses.cmp(&self.root_uses))
            .then_with(|| self.kind.cmp(&other.kind))
            .then_with(|| self.constraint.as_u32().cmp(&other.constraint.as_u32()))
            .then_with(|| self.predicate.as_u32().cmp(&other.predicate.as_u32()))
            .then_with(|| self.sample_roots.cmp(&other.sample_roots))
    }
}

impl PartialOrd for TddHotNodeStat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Histogram bucket keyed by exact interior-node count.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TddHistogramBin {
    pub interior_nodes: usize,
    pub count: usize,
}

/// Aggregate report for TDD size debugging.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct TddStatsReport {
    pub(crate) roots: Vec<TddRootStat>,
    pub(crate) histogram: Vec<TddHistogramBin>,
    pub(crate) hot_nodes: Vec<TddHotNodeStat>,
}

impl TddStatsReport {
    pub(crate) fn from_roots(roots: Vec<TddRootStat>, hot_nodes: Vec<TddHotNodeStat>) -> Self {
        let mut by_size: BTreeMap<usize, usize> = BTreeMap::new();
        for stat in &roots {
            *by_size.entry(stat.interior_nodes).or_default() += 1;
        }
        let histogram = by_size
            .into_iter()
            .map(|(interior_nodes, count)| TddHistogramBin {
                interior_nodes,
                count,
            })
            .collect();
        Self {
            roots,
            histogram,
            hot_nodes,
        }
    }
}

/// Public hot-node summary used by `ty` for reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TddHotNodeSummary {
    pub kind: &'static str,
    pub(crate) constraint_id: ScopedReachabilityConstraintId,
    pub(crate) predicate_id: ScopedPredicateId,
    pub subtree_interior_nodes: usize,
    pub root_uses: usize,
    pub score: usize,
    pub sample_roots: Vec<String>,
}

/// Public, file-level summary used by `ty` for reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTddStatsSummary {
    pub file_path: FilePath,
    pub scopes: Vec<ScopeTddStatsSummary>,
    pub total_roots: usize,
    pub total_interior_nodes: usize,
    pub max_interior_nodes: usize,
    pub reachability_roots: usize,
    pub reachability_interior_nodes: usize,
    pub narrowing_roots: usize,
    pub narrowing_interior_nodes: usize,
}

/// Public, scope-level summary used by `ty` for reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeTddStatsSummary {
    pub scope_id: FileScopeId,
    pub root_count: usize,
    pub total_interior_nodes: usize,
    pub max_interior_nodes: usize,
    pub reachability_roots: usize,
    pub reachability_interior_nodes: usize,
    pub narrowing_roots: usize,
    pub narrowing_interior_nodes: usize,
    pub histogram: Vec<TddHistogramBin>,
    pub hot_nodes: Vec<TddHotNodeSummary>,
}

impl Ord for TddHotNodeSummary {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .cmp(&self.score)
            .then_with(|| {
                other
                    .subtree_interior_nodes
                    .cmp(&self.subtree_interior_nodes)
            })
            .then_with(|| other.root_uses.cmp(&self.root_uses))
            .then_with(|| self.kind.cmp(other.kind))
            .then_with(|| {
                self.constraint_id
                    .as_u32()
                    .cmp(&other.constraint_id.as_u32())
            })
            .then_with(|| self.predicate_id.cmp(&other.predicate_id))
            .then_with(|| self.sample_roots.cmp(&other.sample_roots))
    }
}

impl PartialOrd for TddHotNodeSummary {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScopeTddStatsSummary {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .total_interior_nodes
            .cmp(&self.total_interior_nodes)
            .then_with(|| other.max_interior_nodes.cmp(&self.max_interior_nodes))
            .then_with(|| other.root_count.cmp(&self.root_count))
            .then_with(|| {
                other
                    .reachability_interior_nodes
                    .cmp(&self.reachability_interior_nodes)
            })
            .then_with(|| {
                other
                    .narrowing_interior_nodes
                    .cmp(&self.narrowing_interior_nodes)
            })
            .then_with(|| other.reachability_roots.cmp(&self.reachability_roots))
            .then_with(|| other.narrowing_roots.cmp(&self.narrowing_roots))
            .then_with(|| self.scope_id.as_u32().cmp(&other.scope_id.as_u32()))
            .then_with(|| self.histogram.cmp(&other.histogram))
            .then_with(|| self.hot_nodes.cmp(&other.hot_nodes))
    }
}

impl PartialOrd for ScopeTddStatsSummary {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FileTddStatsSummary {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .total_interior_nodes
            .cmp(&self.total_interior_nodes)
            .then_with(|| other.max_interior_nodes.cmp(&self.max_interior_nodes))
            .then_with(|| other.total_roots.cmp(&self.total_roots))
            .then_with(|| {
                other
                    .reachability_interior_nodes
                    .cmp(&self.reachability_interior_nodes)
            })
            .then_with(|| {
                other
                    .narrowing_interior_nodes
                    .cmp(&self.narrowing_interior_nodes)
            })
            .then_with(|| other.reachability_roots.cmp(&self.reachability_roots))
            .then_with(|| other.narrowing_roots.cmp(&self.narrowing_roots))
            .then_with(|| self.file_path.as_str().cmp(other.file_path.as_str()))
            .then_with(|| self.scopes.cmp(&other.scopes))
    }
}

impl PartialOrd for FileTddStatsSummary {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
