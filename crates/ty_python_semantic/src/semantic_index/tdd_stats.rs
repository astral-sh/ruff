use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::node_key::NodeKey;
use crate::semantic_index::reachability_constraints::ScopedReachabilityConstraintId;

/// TDD root that we want to measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TddRootKind {
    NodeReachability,
}

/// Debug reference to a measured root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TddRootRef {
    pub(crate) kind: TddRootKind,
    pub(crate) node: NodeKey,
    pub(crate) constraint: ScopedReachabilityConstraintId,
}

/// Size stats for one root constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TddRootStat {
    pub(crate) root: TddRootRef,
    pub(crate) interior_nodes: usize,
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
}

impl TddStatsReport {
    pub(crate) fn from_roots(roots: Vec<TddRootStat>) -> Self {
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
        Self { roots, histogram }
    }
}

/// Public, file-level summary used by `ty` for reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTddStatsSummary {
    pub file_path: String,
    pub scopes: Vec<ScopeTddStatsSummary>,
    pub total_roots: usize,
    pub total_interior_nodes: usize,
    pub max_interior_nodes: usize,
}

/// Public, scope-level summary used by `ty` for reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeTddStatsSummary {
    pub scope_id: u32,
    pub root_count: usize,
    pub total_interior_nodes: usize,
    pub max_interior_nodes: usize,
    pub histogram: Vec<TddHistogramBin>,
}

impl Ord for ScopeTddStatsSummary {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .total_interior_nodes
            .cmp(&self.total_interior_nodes)
            .then_with(|| other.max_interior_nodes.cmp(&self.max_interior_nodes))
            .then_with(|| other.root_count.cmp(&self.root_count))
            .then_with(|| self.scope_id.cmp(&other.scope_id))
            .then_with(|| self.histogram.cmp(&other.histogram))
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
            .then_with(|| self.file_path.cmp(&other.file_path))
            .then_with(|| self.scopes.cmp(&other.scopes))
    }
}

impl PartialOrd for FileTddStatsSummary {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
