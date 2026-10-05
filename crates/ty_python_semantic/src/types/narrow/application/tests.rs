use smallvec::smallvec;

use super::super::{
    CombinedNarrowingConstraint, Conjunctions, NarrowingConstraint, NarrowingConstraintKind,
    NarrowingOperation,
};
use crate::types::Type;

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum Shape {
    Empty,
    Intersection,
    Replacement,
    SingletonFiltering,
    SpilledNever,
    Retained,
    AtomicObject,
    ReplacementFirst,
    Pair,
}

pub(in crate::types) fn constraint<'db>(shape: Shape) -> NarrowingConstraint<'db> {
    match shape {
        Shape::Empty => NarrowingConstraint::default(),
        Shape::Intersection => NarrowingConstraint::intersection(Type::AlwaysTruthy),
        Shape::Replacement => NarrowingConstraint::replacement(Type::AlwaysTruthy),
        Shape::SingletonFiltering => NarrowingConstraint(NarrowingConstraintKind::Combined(
            Box::new(CombinedNarrowingConstraint {
                replacement_disjuncts: smallvec![Conjunctions {
                    conjuncts: smallvec![NarrowingOperation::GenericFiltering(Type::AlwaysTruthy)],
                }],
                intersection_disjuncts: smallvec![],
            }),
        )),
        Shape::SpilledNever => NarrowingConstraint(NarrowingConstraintKind::Combined(Box::new(
            CombinedNarrowingConstraint {
                replacement_disjuncts: (0..16)
                    .map(|_| Conjunctions::singleton(Type::Never))
                    .collect(),
                intersection_disjuncts: (0..16)
                    .map(|_| Conjunctions::singleton(Type::Never))
                    .collect(),
            },
        ))),
        Shape::Retained => NarrowingConstraint(NarrowingConstraintKind::Combined(Box::new(
            CombinedNarrowingConstraint {
                replacement_disjuncts: [
                    Conjunctions::singleton(Type::AlwaysTruthy),
                    Conjunctions {
                        conjuncts: smallvec![NarrowingOperation::Intersection(Type::Never); 3],
                    },
                ]
                .into_iter()
                .chain((0..16).map(|_| Conjunctions::singleton(Type::Never)))
                .collect(),
                intersection_disjuncts: smallvec![],
            },
        ))),
        Shape::AtomicObject => NarrowingConstraint::intersection(Type::object()),
        Shape::ReplacementFirst => NarrowingConstraint::from_disjuncts(
            smallvec![Conjunctions::singleton(Type::object())],
            smallvec![Conjunctions {
                conjuncts: smallvec![
                    NarrowingOperation::GenericFiltering(Type::AlwaysTruthy),
                    NarrowingOperation::Intersection(Type::AlwaysFalsy),
                ],
            }],
        ),
        Shape::Pair => NarrowingConstraint::intersection(Type::AlwaysTruthy)
            .merge_constraint_and(NarrowingConstraint::intersection(Type::AlwaysFalsy)),
    }
}
