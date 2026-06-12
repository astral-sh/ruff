use super::{InnerIntersectionBuilder, IntersectionBuilder};
use crate::Db;
use crate::types::Type;
use crate::types::UnionType;
use crate::types::tuple::{FixedLengthTuple, TupleSpec, TupleSpecBuilder, TupleType};
use crate::types::visitor::any_over_type;
use rustc_hash::FxHashSet;

/// Stop simplifying when excluding protocols would create more tuple alternatives than this.
const MAX_INDEXED_PROTOCOL_COMPLEMENT_ALTERNATIVES: usize = 64;

/// Stop simplifying when the generated tuple alternatives would contain more elements than this.
const MAX_INDEXED_PROTOCOL_COMPLEMENT_ELEMENTS: usize = 4_096;

/// Return `true` if none of the element types contain a dynamic type such as `Any`.
///
/// The visitor follows aliases because a dynamic type may be hidden inside one.
fn all_elements_are_static(db: &dyn Db, elements: &[Type<'_>]) -> bool {
    elements
        .iter()
        .all(|element| !any_over_type(db, *element, true, |ty| ty.is_dynamic()))
}

/// Build an ordinary intersection needed by the indexed-protocol simplifier.
///
/// This skips indexed-protocol exclusion so that simplifying one tuple element does not recursively
/// start another protocol expansion.
fn build_indexed_protocol_simplification_intersection<'db>(
    db: &'db dyn Db,
    positives: impl IntoIterator<Item = Type<'db>>,
    negatives: impl IntoIterator<Item = Type<'db>>,
) -> Type<'db> {
    let mut builder = IntersectionBuilder::new(db).positive_elements(positives);
    for negative in negatives {
        builder = builder.add_negative(negative);
    }
    build_without_indexed_protocol_simplification(builder)
}

/// Finish building an intersection after simplifying fixed-length sequence protocols.
pub(super) fn build(builder: IntersectionBuilder<'_>) -> Type<'_> {
    build_without_indexed_protocol_simplification(builder.simplify_indexed_protocol_negatives())
}

/// Finish building an intersection without starting another indexed-protocol pass.
fn build_without_indexed_protocol_simplification(builder: IntersectionBuilder<'_>) -> Type<'_> {
    UnionType::from_elements(
        builder.db,
        builder
            .intersections
            .into_iter()
            .map(|inner| inner.build(builder.db)),
    )
}

/// What remains after excluding a fixed-length sequence protocol from an exact tuple.
enum TupleProtocolComplement<'db> {
    /// The tuple cannot satisfy the protocol, so excluding it changes nothing.
    Redundant,
    /// Every value in the tuple satisfies the protocol, so excluding it removes the whole branch.
    Eliminated,
    /// Ways to violate the protocol, represented by the changed index and its remaining type.
    Alternatives(Vec<(usize, Type<'db>)>),
}

/// A synthesized fixed-length sequence protocol and the type required at each index.
struct IndexedProtocolConstraint<'db> {
    protocol: Type<'db>,
    elements: Box<[Type<'db>]>,
}

/// The result of intersecting all exact tuple types in one branch.
///
/// `positive_indices` identifies the original tuple constraints that this combined tuple replaces.
struct StaticTupleIntersection<'db> {
    positive_indices: Vec<usize>,
    tuple: TupleSpec<'db>,
}

/// A plan for replacing one intersection branch with exact tuple alternatives.
///
/// Applying the plan removes `handled_protocols` and the tuple constraints at `positive_indices`,
/// then creates one branch for each entry in `alternatives`.
struct IndexedProtocolExpansionPlan<'db> {
    handled_protocols: Vec<Type<'db>>,
    positive_indices: Vec<usize>,
    alternatives: Vec<Vec<Type<'db>>>,
}

/// Combine a tuple's element types with the element types required by a sequence protocol.
///
/// Return `None` if `ty` is not a tuple or contains dynamic element types. Return `Never` if the
/// tuple cannot satisfy the protocol.
///
/// This assumes that tuple subclasses iterate over the same values returned by indexing. A
/// subclass can break that assumption by overriding `__iter__`, but keeping the assumption gives
/// precise results for ordinary tuple annotations.
fn refine_tuple_with_indexed_protocol<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    protocol_elements: &[Type<'db>],
) -> Option<Type<'db>> {
    if !all_elements_are_static(db, protocol_elements) {
        return None;
    }

    let Type::NominalInstance(instance) = ty else {
        return None;
    };

    let tuple = instance.own_tuple_spec(db)?;
    if !all_elements_are_static(db, tuple.all_elements()) {
        return None;
    }

    let protocol_tuple = TupleSpec::heterogeneous(protocol_elements.iter().copied());
    let Some(refined) = TupleSpecBuilder::from(tuple.as_ref()).intersect(db, &protocol_tuple)
    else {
        return Some(Type::Never);
    };

    Some(Type::tuple(TupleType::new(db, &refined.build())))
}

/// Replace a tuple/protocol intersection with a tuple whose element types include the protocol's
/// requirements.
///
/// For example:
///
/// ```python
/// def narrow(value: tuple[int | str, int | str]) -> None:
///     match value:
///         case [_, str()]:
///             reveal_type(value)  # tuple[int | str, str]
/// ```
pub(super) fn refine_tuple_protocol_intersection<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
) -> Option<Type<'db>> {
    let ((Type::ProtocolInstance(protocol), tuple) | (tuple, Type::ProtocolInstance(protocol))) =
        (left, right)
    else {
        return None;
    };
    let indexed = protocol.finite_indexed_constraint(db)?;
    refine_tuple_with_indexed_protocol(db, tuple, &indexed)
}

/// Find the ways an exact tuple can fail to satisfy a fixed-length sequence protocol.
///
/// Each returned alternative changes one tuple position to the part of its type that the protocol
/// rejects. Return an error instead of producing more than `max_alternatives` alternatives.
fn tuple_protocol_complement<'db>(
    db: &'db dyn Db,
    tuple: &FixedLengthTuple<Type<'db>>,
    protocol_elements: &[Type<'db>],
    max_alternatives: usize,
) -> Result<TupleProtocolComplement<'db>, ()> {
    if tuple.len() != protocol_elements.len() {
        return Ok(TupleProtocolComplement::Redundant);
    }

    let mut alternatives = Vec::new();
    for (index, (element, protocol_element)) in tuple
        .all_elements()
        .iter()
        .zip(protocol_elements)
        .enumerate()
    {
        if element.is_disjoint_from(db, *protocol_element) {
            return Ok(TupleProtocolComplement::Redundant);
        }
        if element.is_subtype_of(db, *protocol_element) {
            continue;
        }

        let remaining_element =
            build_indexed_protocol_simplification_intersection(db, [*element], [*protocol_element]);
        if remaining_element.is_never() {
            continue;
        }
        if alternatives.len() == max_alternatives {
            return Err(());
        }
        alternatives.push((index, remaining_element));
    }

    Ok(if alternatives.is_empty() {
        TupleProtocolComplement::Eliminated
    } else {
        TupleProtocolComplement::Alternatives(alternatives)
    })
}

impl<'db> IntersectionBuilder<'db> {
    /// Simplify exclusions of fixed-length sequence protocols from exact tuple types.
    ///
    /// Exclusions that change only one tuple position are applied first. Exclusions that can fail
    /// at several positions create separate tuple branches. If that would create too many branches,
    /// those exclusions are left unchanged.
    fn simplify_indexed_protocol_negatives(mut self) -> Self {
        let protocols = self.indexed_protocol_constraints();
        if protocols.is_empty() {
            return self;
        }

        let db = self.db;
        self.intersections = self
            .intersections
            .into_iter()
            .filter_map(|inner| inner.simplify_indexed_protocol_negatives(db, &protocols))
            .collect();

        let Ok(plans) = self
            .intersections
            .iter()
            .map(|inner| inner.plan_indexed_protocol_expansion(db, &protocols))
            .collect::<Result<Vec<_>, ()>>()
        else {
            return self;
        };

        let mut total_alternatives = 0usize;
        let mut total_elements = 0usize;
        for plan in &plans {
            total_alternatives = total_alternatives
                .saturating_add(plan.as_ref().map_or(1, |plan| plan.alternatives.len()));
            total_elements = total_elements.saturating_add(
                plan.as_ref()
                    .map_or(0, |plan| plan.alternatives.iter().map(Vec::len).sum()),
            );
            if total_alternatives > MAX_INDEXED_PROTOCOL_COMPLEMENT_ALTERNATIVES
                || total_elements > MAX_INDEXED_PROTOCOL_COMPLEMENT_ELEMENTS
            {
                return self;
            }
        }

        self.intersections = self
            .intersections
            .into_iter()
            .zip(plans)
            .flat_map(|(inner, plan)| match plan {
                Some(plan) => inner.apply_indexed_protocol_expansion(db, plan),
                None => vec![inner],
            })
            .collect();
        self
    }

    /// Collect the fixed-length sequence protocols excluded by any branch.
    ///
    /// Ignore protocols with dynamic element types. Sort the remaining protocols so the result
    /// does not depend on the order in which constraints were added.
    fn indexed_protocol_constraints(&self) -> Vec<IndexedProtocolConstraint<'db>> {
        let mut seen_protocols = FxHashSet::default();
        let mut constraints = Vec::new();

        for negative in self
            .intersections
            .iter()
            .flat_map(|inner| inner.negative.iter())
        {
            let Type::ProtocolInstance(protocol) = negative else {
                continue;
            };
            if !seen_protocols.insert(*negative) {
                continue;
            }
            let Some(elements) = protocol.finite_indexed_constraint(self.db) else {
                continue;
            };
            if !all_elements_are_static(self.db, &elements) {
                continue;
            }
            constraints.push(IndexedProtocolConstraint {
                protocol: *negative,
                elements,
            });
        }

        constraints.sort_by_cached_key(|constraint| {
            constraint
                .elements
                .iter()
                .map(|element| element.display(self.db).to_string())
                .collect::<Box<[_]>>()
        });
        constraints
    }
}

impl<'db> InnerIntersectionBuilder<'db> {
    /// Combine all exact tuple constraints in this branch into one tuple.
    ///
    /// The returned indices identify which original constraints should be replaced by the result.
    fn static_tuple_intersection(
        &self,
        db: &'db dyn Db,
    ) -> Result<Option<StaticTupleIntersection<'db>>, ()> {
        let mut positive_indices = Vec::new();
        let mut intersection: Option<TupleSpecBuilder<'db>> = None;

        for (positive_index, positive) in self.positive.iter().enumerate() {
            let Type::NominalInstance(instance) = positive else {
                continue;
            };
            let Some(tuple) = instance.own_tuple_spec(db) else {
                continue;
            };
            if !all_elements_are_static(db, tuple.all_elements()) {
                continue;
            }

            positive_indices.push(positive_index);
            intersection = Some(match intersection {
                Some(intersection) => intersection.intersect(db, tuple.as_ref()).ok_or(())?,
                None => TupleSpecBuilder::from(tuple.as_ref()),
            });
        }

        let Some(intersection) = intersection else {
            return Ok(None);
        };
        let intersection = intersection.build();
        if let TupleSpec::Fixed(tuple) = &intersection
            && tuple.all_elements().iter().any(Type::is_never)
        {
            return Err(());
        }
        Ok(Some(StaticTupleIntersection {
            positive_indices,
            tuple: intersection,
        }))
    }

    /// Apply protocol exclusions that change at most one tuple position.
    ///
    /// Remove exclusions that already have no effect. Return `None` if an exclusion makes the
    /// branch impossible. Leave exclusions with several alternatives for the next step.
    fn simplify_indexed_protocol_negatives(
        mut self,
        db: &'db dyn Db,
        protocols: &[IndexedProtocolConstraint<'db>],
    ) -> Option<Self> {
        'fixed_point: loop {
            let intersection = match self.static_tuple_intersection(db) {
                Ok(Some(intersection)) => intersection,
                Ok(None) => return Some(self),
                Err(()) => return None,
            };
            let TupleSpec::Fixed(tuple) = &intersection.tuple else {
                return Some(self);
            };

            for constraint in protocols {
                if !self.negative.contains(&constraint.protocol) {
                    continue;
                }

                match tuple_protocol_complement(db, tuple, &constraint.elements, 2) {
                    Err(()) => continue,
                    Ok(TupleProtocolComplement::Redundant) => {
                        self.negative.swap_remove(&constraint.protocol);
                    }
                    Ok(TupleProtocolComplement::Eliminated) => return None,
                    Ok(TupleProtocolComplement::Alternatives(alternatives)) => {
                        let [(index, remaining_element)] = alternatives.as_slice() else {
                            continue;
                        };
                        let mut elements = tuple.all_elements().to_vec();
                        elements[*index] = *remaining_element;
                        for positive_index in intersection.positive_indices.into_iter().rev() {
                            self.positive.swap_remove_index(positive_index);
                        }
                        self.add_positive(db, Type::heterogeneous_tuple(db, elements));
                        if self.positive.contains(&Type::Never) {
                            return None;
                        }
                        self.negative.swap_remove(&constraint.protocol);
                        continue 'fixed_point;
                    }
                }
            }

            return Some(self);
        }
    }

    /// Work out the tuple branches needed for the remaining protocol exclusions.
    ///
    /// This does not change the branch. Return an error if the result would exceed the branch or
    /// tuple-element limits.
    fn plan_indexed_protocol_expansion(
        &self,
        db: &'db dyn Db,
        protocols: &[IndexedProtocolConstraint<'db>],
    ) -> Result<Option<IndexedProtocolExpansionPlan<'db>>, ()> {
        let Ok(Some(intersection)) = self.static_tuple_intersection(db) else {
            return Ok(None);
        };
        let TupleSpec::Fixed(tuple) = &intersection.tuple else {
            return Ok(None);
        };

        let mut handled_protocols = Vec::new();
        let mut alternatives = vec![tuple.all_elements().to_vec()];

        for constraint in protocols {
            if !self.negative.contains(&constraint.protocol) {
                continue;
            }
            let Ok(TupleProtocolComplement::Alternatives(tuple_alternatives)) =
                tuple_protocol_complement(
                    db,
                    tuple,
                    &constraint.elements,
                    MAX_INDEXED_PROTOCOL_COMPLEMENT_ALTERNATIVES,
                )
            else {
                continue;
            };
            if tuple_alternatives.len() < 2 {
                continue;
            }

            let mut expanded = Vec::new();
            let mut seen = FxHashSet::default();
            for elements in &alternatives {
                for (index, remaining_element) in &tuple_alternatives {
                    let mut elements = elements.clone();
                    let element = build_indexed_protocol_simplification_intersection(
                        db,
                        [elements[*index], *remaining_element],
                        [],
                    );
                    if element.is_never() {
                        continue;
                    }
                    elements[*index] = element;
                    if !seen.insert(elements.clone()) {
                        continue;
                    }
                    if expanded.len() == MAX_INDEXED_PROTOCOL_COMPLEMENT_ALTERNATIVES
                        || expanded
                            .len()
                            .saturating_add(1)
                            .saturating_mul(elements.len())
                            > MAX_INDEXED_PROTOCOL_COMPLEMENT_ELEMENTS
                    {
                        return Err(());
                    }
                    expanded.push(elements);
                }
            }

            handled_protocols.push(constraint.protocol);
            alternatives = expanded;
            if alternatives.is_empty() {
                break;
            }
        }

        if handled_protocols.is_empty() {
            return Ok(None);
        }
        Ok(Some(IndexedProtocolExpansionPlan {
            handled_protocols,
            positive_indices: intersection.positive_indices,
            alternatives,
        }))
    }

    /// Replace this branch with the tuple alternatives in `plan`.
    ///
    /// Keep constraints unrelated to the handled protocols and tuples. Drop any alternative that
    /// becomes impossible after it is added back to the branch.
    fn apply_indexed_protocol_expansion(
        mut self,
        db: &'db dyn Db,
        plan: IndexedProtocolExpansionPlan<'db>,
    ) -> Vec<Self> {
        for protocol in &plan.handled_protocols {
            self.negative.swap_remove(protocol);
        }
        for positive_index in plan.positive_indices.iter().rev() {
            self.positive.swap_remove_index(*positive_index);
        }

        let mut alternatives = Vec::with_capacity(plan.alternatives.len());
        for elements in plan.alternatives {
            let mut alternative = self.clone();
            alternative.add_positive(db, Type::heterogeneous_tuple(db, elements));
            if !alternative.positive.contains(&Type::Never) {
                alternatives.push(alternative);
            }
        }
        alternatives
    }
}
