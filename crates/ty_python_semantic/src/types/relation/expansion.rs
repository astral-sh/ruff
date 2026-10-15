//! Consume deferred bodies while the comparison that opened them remains active.

use crate::Db;
use crate::types::callable::{CallableTypes, SignatureSource};
use crate::types::constraints::{ConstraintSet, IteratorConstraintsExtension};
use crate::types::recursive::{RecursiveType, TypeExpansionContext};
use crate::types::type_alias::TypeAliasType;
use crate::types::typevar::TypeVarSet;
use crate::types::{CallableType, Type};

use super::{ComparisonKind, ComparisonPolicy, DisjointnessChecker, TypeRelationChecker};

/// A relation whose pair guard has already been installed.
///
/// Expansion returns a comparison result, never an alias body or signature. This keeps the
/// expanded values inside the comparison that owns their recursion state.
/// Union branches retain the other operand's original reference: comparing an alias's body
/// instead can lose the stable pair needed to detect growing recursive specializations.
pub(in crate::types) struct GuardedRelation<'a, 'c, 'db> {
    checker: &'a TypeRelationChecker<'a, 'c, 'db>,
    expansion: &'a TypeExpansionContext<'a, 'a, 'db>,
    references: (Type<'db>, Type<'db>),
}

impl<'a, 'c, 'db> GuardedRelation<'a, 'c, 'db> {
    pub(super) fn new(
        checker: &'a TypeRelationChecker<'a, 'c, 'db>,
        expansion: &'a TypeExpansionContext<'a, 'a, 'db>,
        references: (Type<'db>, Type<'db>),
    ) -> Self {
        Self {
            checker,
            expansion,
            references,
        }
    }

    pub(super) fn source_reference(&self) -> Type<'db> {
        self.references.0
    }

    pub(super) fn target_reference(&self) -> Type<'db> {
        self.references.1
    }

    pub(in crate::types) fn compare_expanded_pair(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.checker
            .check_type_pair_with_references(db, source, target, self.references)
    }

    pub(super) fn needs_intersection_expansion(
        &self,
        db: &'db dyn Db,
        intersection: crate::types::IntersectionType<'db>,
    ) -> bool {
        self.expansion
            .needs_intersection_expansion(db, intersection)
    }

    pub(super) fn compare_source_intersection(
        &self,
        db: &'db dyn Db,
        source: crate::types::IntersectionType<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let body = self.expansion.comparison_intersection_body(db, source);
        if body.has_same_type(Type::Intersection(source)) {
            // Normalization can retain a growing recursive reference. Continue dispatching
            // under this pair's guard rather than treating normalization as a recursive proof.
            self.checker
                .check_type_pair_guarded(db, self, Type::Intersection(source), target)
        } else {
            body.relate_as_source(db, self, target)
        }
    }

    pub(super) fn compare_target_intersection(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: crate::types::IntersectionType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let body = self.expansion.comparison_intersection_body(db, target);
        if body.has_same_type(Type::Intersection(target)) {
            self.checker
                .check_type_pair_guarded(db, self, source, Type::Intersection(target))
        } else {
            body.relate_as_target(db, self, source)
        }
    }

    pub(super) fn compare_source_alias(
        &self,
        db: &'db dyn Db,
        source: TypeAliasType<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.expansion
            .comparison_alias_body(db, source)
            .relate_as_source(db, self, target)
    }

    pub(super) fn compare_target_alias(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: TypeAliasType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.expansion
            .comparison_alias_body(db, target)
            .relate_as_target(db, self, source)
    }

    pub(super) fn compare_source_recursive(
        &self,
        db: &'db dyn Db,
        source: RecursiveType<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let by_arguments = if let Type::Recursive(target) = target {
            self.checker
                .when_recursive_types_relate_by_arguments(db, source, target)
        } else {
            self.checker.never()
        };
        by_arguments.or(db, self.checker.constraints, || {
            self.expansion
                .comparison_recursive_body(db, source)
                .map(|source| source.relate_as_source(db, self, target))
                .unwrap_or(ConstraintSet::from_bool(
                    self.checker.constraints,
                    self.checker.relation.is_assignability(),
                ))
        })
    }

    pub(super) fn compare_target_recursive(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: RecursiveType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.expansion
            .comparison_recursive_body(db, target)
            .map(|target| target.relate_as_target(db, self, source))
            .unwrap_or(ConstraintSet::from_bool(
                self.checker.constraints,
                self.checker.relation.is_assignability(),
            ))
    }

    pub(super) fn compare_signatures(
        &self,
        db: &'db dyn Db,
        source: CallableType<'db>,
        target: CallableType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.expansion
            .inspect_signatures(db, source, |source| {
                self.expansion
                    .inspect_signatures(db, target, |target| {
                        source.compare(db, &target, self.checker)
                    })
                    .unwrap_or_else(|| self.checker.never())
            })
            .unwrap_or_else(|| self.checker.never())
    }
}

/// Disjointness has a different cycle policy from subtyping, but the same expansion boundary.
pub(super) struct GuardedDisjointness<'a, 'c, 'db> {
    checker: &'a DisjointnessChecker<'a, 'c, 'db>,
    expansion: &'a TypeExpansionContext<'a, 'a, 'db>,
}

impl<'a, 'c, 'db> GuardedDisjointness<'a, 'c, 'db> {
    pub(super) fn new(
        checker: &'a DisjointnessChecker<'a, 'c, 'db>,
        expansion: &'a TypeExpansionContext<'a, 'a, 'db>,
    ) -> Self {
        Self { checker, expansion }
    }

    pub(super) fn compare_source_alias(
        &self,
        db: &'db dyn Db,
        source: TypeAliasType<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.expansion
            .comparison_alias_body(db, source)
            .disjoint_as_source(db, self.checker, target)
    }

    pub(super) fn compare_target_alias(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: TypeAliasType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.expansion
            .comparison_alias_body(db, target)
            .disjoint_as_target(db, self.checker, source)
    }

    pub(super) fn compare_source_recursive(
        &self,
        db: &'db dyn Db,
        source: RecursiveType<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.expansion
            .comparison_recursive_body(db, source)
            .map(|source| source.disjoint_as_source(db, self.checker, target))
            .unwrap_or(self.checker.never())
    }

    pub(super) fn compare_target_recursive(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: RecursiveType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.expansion
            .comparison_recursive_body(db, target)
            .map(|target| target.disjoint_as_target(db, self.checker, source))
            .unwrap_or(self.checker.never())
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    /// Compare saved inputs before expanding two views of the same lambda.
    pub(super) fn equivalent_lambda_inputs(
        &self,
        db: &'db dyn Db,
        source_callable: CallableType<'db>,
        target_callable: CallableType<'db>,
    ) -> bool {
        if self.relation.is_subtyping()
            || source_callable.runtime_class(db) != target_callable.runtime_class(db)
        {
            return false;
        }
        let (SignatureSource::Lambda(source), SignatureSource::Lambda(target)) = (
            source_callable.signature_source(db),
            target_callable.signature_source(db),
        ) else {
            return false;
        };
        self.materialization_visitor
            .recursion_context()
            .expand_signature(db, self.env, source_callable, || {
                source.has_equivalent_signature(db, self.env, *target, |left, right| {
                    self.as_equivalence_checker()
                        .check_type_pair(db, left, right)
                        .is_always_satisfied(db, self.env, TypeVarSet::None)
                })
            })
            .unwrap_or(false)
    }

    /// Check whether one callable type has the given relation to another callable type.
    ///
    /// See [`Type::is_subtype_of`] and [`Type::is_assignable_to`] for more details.
    pub(in crate::types) fn check_callable_pair(
        &self,
        db: &'db dyn Db,
        source: CallableType<'db>,
        target: CallableType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_type_pair(db, Type::Callable(source), Type::Callable(target))
    }

    pub(super) fn check_callable_pair_inner(
        &self,
        db: &'db dyn Db,
        source: CallableType<'db>,
        target: CallableType<'db>,
        comparison: &GuardedRelation<'_, 'c, 'db>,
    ) -> ConstraintSet<'db, 'c> {
        if target.runtime_class(db).is_some()
            && target.runtime_class(db) != source.runtime_class(db)
        {
            return self.never();
        }
        if self.equivalent_lambda_inputs(db, source, target) {
            return self.always();
        }
        comparison.compare_signatures(db, source, target)
    }

    pub(in crate::types) fn check_callables_vs_callable(
        &self,
        db: &'db dyn Db,
        source: &CallableTypes<'db>,
        target: CallableType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        source.iter().when_all(db, self.constraints, |element| {
            self.check_callable_pair(db, *element, target)
        })
    }
}

impl<'c, 'db> DisjointnessChecker<'_, 'c, 'db> {
    /// Compare method return types without exposing either expanded signature to the caller.
    pub(in crate::types) fn check_callable_return_pair(
        &self,
        db: &'db dyn Db,
        source: CallableType<'db>,
        target: CallableType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.env
            .context()
            .state()
            .comparisons
            .get_or_init(Box::default)
            .visit(
                db,
                self.env,
                (Type::Callable(source), Type::Callable(target)),
                ComparisonPolicy {
                    kind: ComparisonKind::CallableReturns,
                    inferable: self.inferable,
                    provenance: self.provenance,
                    perform_expensive_checks: self.perform_expensive_checks,
                },
                |cycle| {
                    if cycle.is_some() {
                        return self.never();
                    }
                    self.disjointness_visitor
                        .callable_returns
                        .visit_active_with_expansion(
                            db,
                            (Type::Callable(source), Type::Callable(target)),
                            self.materialization_visitor,
                            |expansion| match expansion {
                                Ok(expansion) => GuardedDisjointness::new(self, expansion)
                                    .compare_callable_returns(db, source, target),
                                Err(_) => self.never(),
                            },
                        )
                },
            )
    }
}

impl<'c, 'db> GuardedDisjointness<'_, 'c, 'db> {
    fn compare_callable_returns(
        &self,
        db: &'db dyn Db,
        source: CallableType<'db>,
        target: CallableType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let checker = self.checker;
        let env = checker.env;
        // Keep signature substitution active while checking the returns. The pair guard above
        // belongs to disjointness; the signature guards also cover nested single-type queries.
        source
            .inspect_signatures(db, env, |source| {
                if !source.has_only_non_never_returns(db) {
                    return checker.never();
                }
                target
                    .inspect_signatures(db, env, |target| {
                        if !target.has_only_non_never_returns(db) {
                            return checker.never();
                        }
                        source.compare_return_disjointness(db, &target, checker)
                    })
                    .unwrap_or_else(|| checker.never())
            })
            .unwrap_or_else(|| checker.never())
    }
}
