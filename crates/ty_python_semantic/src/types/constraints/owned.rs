use std::sync::Arc;

use ruff_index::{Idx, IndexVec};
use rustc_hash::FxHashMap;
use ty_python_core::rank::{RankBitBox, RankBitBoxVec};

use crate::types::constraints::support::Support;
use crate::types::constraints::variables::{Constraint, ExistentialBound};
use crate::types::constraints::{
    ConstraintId, ConstraintSetStorage, InteriorNode, NodeId, OwnedConstraintSet,
    OwnedConstraintSetInner, SourceOrder, SourceOrderId, SupportId,
};

pub(super) struct OwnedConstraintSetBuilder {
    used_nodes: RankBitBoxVec,
    used_constraints: RankBitBoxVec,
    used_supports: RankBitBoxVec,
    live_support: Option<Support>,
    source_orders: IndexVec<SourceOrderId, SourceOrder>,
    mapped_source_orders: FxHashMap<SourceOrderId, Option<SourceOrderId>>,
}

impl OwnedConstraintSetBuilder {
    pub(super) fn build(
        mut storage: ConstraintSetStorage<'_>,
        root: InteriorNode,
        source_order: SourceOrderId,
    ) -> OwnedConstraintSet<'_> {
        let mut builder = Self {
            used_nodes: RankBitBox::bits_with_capacity(storage.nodes.len()),
            used_constraints: RankBitBox::bits_with_capacity(storage.constraints.len()),
            used_supports: RankBitBox::bits_with_capacity(storage.supports.len()),
            live_support: storage.node_support(root.node()).cloned(),
            source_orders: IndexVec::default(),
            mapped_source_orders: FxHashMap::default(),
        };
        builder.mark_node_used(&mut storage, root.node());
        let mapped_source_order = builder
            .mark_source_order_used(&mut storage, source_order)
            .expect("non-terminal BDD should have source_order");
        builder.finish(storage, root, mapped_source_order)
    }

    fn mark_node_used(&mut self, storage: &mut ConstraintSetStorage<'_>, node: NodeId) {
        if node.is_terminal() || self.used_nodes[node.index()] {
            return;
        }
        self.used_nodes.set(node.index(), true);

        let node_support = storage
            .node_support_id(node)
            .expect("node should be non-terminal");
        self.mark_support_used(storage, node_support);

        let interior = storage.interior_node_data(node);
        self.mark_constraint_used(storage, interior.constraint);
        self.mark_node_used(storage, interior.if_true);
        self.mark_node_used(storage, interior.if_uncertain);
        self.mark_node_used(storage, interior.if_false);
    }

    fn mark_constraint_used(
        &mut self,
        storage: &mut ConstraintSetStorage<'_>,
        constraint: ConstraintId,
    ) {
        if self.used_constraints[constraint.index()] {
            return;
        }
        self.used_constraints.set(constraint.index(), true);

        let constraint_support = storage.constraint_support_id(constraint);
        self.mark_support_used(storage, constraint_support);

        let constraint_data = storage.constraint_data(constraint);
        match constraint_data {
            Constraint::Atomic(_) => {}
            Constraint::Existential(existential) => {
                let ExistentialBound {
                    body, source_order, ..
                } = *existential;
                self.mark_node_used(storage, body);
                if let Some(source_order) = source_order {
                    let mapped_source_order = self.mark_source_order_used(storage, source_order);
                    self.mapped_source_orders
                        .insert(source_order, mapped_source_order);
                }
            }
        }
    }

    fn mark_support_used(&mut self, _storage: &mut ConstraintSetStorage<'_>, support: SupportId) {
        self.used_supports.set(support.index(), true);
    }

    fn mark_source_order_used(
        &mut self,
        storage: &mut ConstraintSetStorage<'_>,
        source_order: SourceOrderId,
    ) -> Option<SourceOrderId> {
        let source_order_data = storage.source_order_data(source_order);
        match source_order_data {
            SourceOrder::Ordered(left, right) => {
                let mapped_left = self.mark_source_order_used(storage, left);
                let mapped_right = self.mark_source_order_used(storage, right);
                match (mapped_left, mapped_right) {
                    (None, None) => None,
                    (None, other) | (other, None) => other,
                    (Some(left), Some(right)) if left == right => Some(left),
                    (Some(left), Some(right)) => {
                        Some(self.source_orders.push(SourceOrder::Ordered(left, right)))
                    }
                }
            }
            SourceOrder::AtomicConstraint(constraint) => {
                // If a constraint is not used anywhere in the BDD, and doesn't mention any
                // typevars that are used in the BDD, we don't have to include it in the compacted
                // source_order list.
                let constraint_support_id = storage.constraint_support_id(constraint.into_inner());
                let constraint_support = storage.support_data(constraint_support_id);
                if !self.used_constraints[constraint.index()]
                    && let Some(live_support) = self.live_support.as_ref()
                    && live_support.is_complete()
                    && constraint_support.is_complete()
                    && !constraint_support.overlaps_with(live_support)
                {
                    return None;
                }

                self.mark_constraint_used(storage, constraint.into_inner());
                self.mark_support_used(storage, constraint_support_id);
                let mapped = self
                    .source_orders
                    .push(SourceOrder::AtomicConstraint(constraint));
                Some(mapped)
            }
        }
    }

    fn finish(
        mut self,
        mut storage: ConstraintSetStorage<'_>,
        root: InteriorNode,
        mapped_source_order: SourceOrderId,
    ) -> OwnedConstraintSet<'_> {
        let largest = self.used_nodes.last_one().map_or(0, |last| last + 1);
        self.used_nodes.truncate(largest);
        let largest = self.used_constraints.last_one().map_or(0, |last| last + 1);
        self.used_constraints.truncate(largest);
        let largest = self.used_supports.last_one().map_or(0, |last| last + 1);
        self.used_supports.truncate(largest);

        let nodes = storage
            .nodes
            .into_iter()
            .zip(&self.used_nodes)
            .filter_map(|(node, used)| used.then_some(node))
            .collect();
        let node_supports = storage
            .node_supports
            .into_iter()
            .zip(&self.used_nodes)
            .filter_map(|(support, used)| used.then_some(support))
            .collect();
        let node_indices = RankBitBox::from_bits(self.used_nodes);

        let constraints = storage
            .constraints
            .into_iter()
            .zip(&self.used_constraints)
            .filter_map(|(mut constraint, used)| {
                if !used {
                    return None;
                }
                if let Constraint::Existential(existential) = &mut constraint
                    && let Some(source_order) = existential.source_order
                {
                    existential.source_order = self.mapped_source_orders[&source_order];
                }
                Some(constraint)
            })
            .collect();
        let constraint_supports = storage
            .constraint_supports
            .into_iter()
            .zip(&self.used_constraints)
            .filter_map(|(support, used)| used.then_some(support))
            .collect();
        let constraint_indices = RankBitBox::from_bits(self.used_constraints);

        let supports = storage
            .supports
            .into_iter()
            .zip(&self.used_supports)
            .filter_map(|(support, used)| used.then_some(support))
            .collect();
        let support_indices = RankBitBox::from_bits(self.used_supports);

        storage.typevars.shrink_to_fit();

        OwnedConstraintSet {
            node: root.node(),
            source_order: Some(mapped_source_order),
            inner: Some(Arc::new(OwnedConstraintSetInner {
                constraints,
                constraint_supports,
                constraint_indices,
                typevars: storage.typevars,
                nodes,
                node_supports,
                node_indices,
                supports,
                support_indices,
                source_orders: self.source_orders.raw.into_boxed_slice(),
            })),
        }
    }
}
