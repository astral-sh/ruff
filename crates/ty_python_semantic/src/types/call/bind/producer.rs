//! Selects argument contexts for a generic call by solving for the caller's type variables
//! together with the type variables of its argument expressions.
//!
//! For `as_list[T](x: T | list[T])`, a list literal `[1]` supplies `int ≤ E` and has type
//! `list[E]`. Either `list[E] ≤ T` or `list[E] ≤ list[T]` suffices. Passing a
//! `frozenset[E]` to `Intersection[T, Iterable[T]]` requires both `frozenset[E] ≤ T` and
//! `E ≤ T`. The same assignability relation builds both constraint sets.
//!
//! Selecting one feasible `E` provides a context for the next pass over the argument. The
//! ordinary call solver still checks the resulting argument types against the original
//! signature and considers all viable alternatives when inferring the return type.

use rustc_hash::FxHashSet;

use super::{Binding, CallableBinding, matched_argument_relations};
use crate::db::Db;
use crate::types::call::CallArguments;
use crate::types::constraints::{
    ConstraintProvenance, ConstraintSet, ConstraintSetBuilder, OwnedConstraintSet,
    ReportDiagnostics, SolutionPaths, Solutions,
};
use crate::types::generics::{GenericContext, Specialization, SpecializationBuilder};
use crate::types::typevar::{BindingContext, TypeVarNonceGenerator};
use crate::types::visitor::any_over_type_expanding_aliases;
use crate::types::{BoundTypeVarInstance, ProgramEnvironment, Type, TypeContext};

// Opening arguments is optional. Bound the size of the graph we construct before the solver's
// own traversal budgets take effect.
pub(crate) const MAX_LOCAL_TYPEVARS: usize = 16;
pub(crate) const MAX_PRODUCER_RELATIONS: usize = 32;

/// An expression's result before its local type variables have been specialized, together
/// with the argument constraints that any specialization must satisfy.
#[derive(Debug, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct ExpressionConstraints<'db> {
    pub(crate) closed: Type<'db>,
    pub(crate) ty: Type<'db>,
    pub(crate) facts: OwnedConstraintSet<'db>,
    pub(crate) locals: Vec<(BoundTypeVarInstance<'db>, Type<'db>)>,
    has_empty_operand: bool,
}

impl<'db> ExpressionConstraints<'db> {
    pub(crate) fn renaming(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        context: GenericContext<'db>,
        nonces: &TypeVarNonceGenerator<'db>,
    ) -> Specialization<'db> {
        let scope = BindingContext::Synthetic(env.program(db));
        let variables = context.variables(db).map(|var| {
            Type::TypeVar(BoundTypeVarInstance::new(
                db,
                var.typevar(db),
                scope,
                None,
                nonces.next(),
            ))
        });
        context.specialize(db, variables.collect::<Vec<_>>())
    }

    pub(crate) fn new<'a>(
        db: &'db dyn Db,
        closed: Type<'db>,
        ty: Type<'db>,
        facts: OwnedConstraintSet<'db>,
        specialization: Option<(Specialization<'db>, Specialization<'db>)>,
        children: impl IntoIterator<Item = &'a Self>,
        mut has_empty_operand: bool,
    ) -> Option<Self>
    where
        'db: 'a,
    {
        let mut locals = Vec::new();
        if let Some((renaming, intrinsic)) = specialization {
            locals.extend(
                renaming
                    .types(db)
                    .iter()
                    .zip(intrinsic.types(db))
                    .filter_map(|(v, ty)| Some((v.as_typevar()?, *ty))),
            );
        }
        for child in children {
            if locals.len() + child.locals.len() > MAX_LOCAL_TYPEVARS {
                return None;
            }
            locals.extend_from_slice(&child.locals);
            has_empty_operand |= child.has_empty_operand;
        }
        Some(Self {
            closed,
            ty,
            facts,
            locals,
            has_empty_operand,
        })
    }

    pub(crate) fn fixed(ty: Type<'db>) -> Self {
        Self {
            closed: ty,
            ty,
            facts: OwnedConstraintSet::always(),
            locals: Vec::new(),
            has_empty_operand: false,
        }
    }

    pub(crate) fn relate<'c>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arena: &'c ConstraintSetBuilder<'db>,
        formal: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        arena.load(db, env, &self.facts).and(db, arena, || {
            self.ty
                .when_constraint_set_assignable_to(db, env, formal, arena)
        })
    }
}

/// A single constructed value can supply conflicting contexts for its own type arguments.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Feedback {
    None,
    Empty,
    Repeated,
}

impl<'db> ExpressionConstraints<'db> {
    fn feedback(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        outer: GenericContext<'db>,
        parameter: Type<'db>,
        actual: Type<'db>,
    ) -> Feedback {
        if let Some(formals) = parameter
            .resolve_type_alias(db)
            .exact_tuple_instance_spec(db)
            && let Some(actuals) = actual.exact_tuple_instance_spec(db)
            && let (Some(formals), Some(actuals)) =
                (formals.as_fixed_length(), actuals.as_fixed_length())
            && formals.len() == actuals.len()
        {
            // Each element receives its own expected type. A variable shared between separate
            // tuple elements supplies ordinary context to each of them.
            return formals
                .iter_all_elements()
                .zip(actuals.iter_all_elements())
                .map(|(formal, actual)| self.feedback(db, env, outer, formal, actual))
                .max()
                .unwrap_or(Feedback::None);
        }
        let mut variables = FxHashSet::default();
        let mut repeated = false;
        parameter.visit_specialization(db, env, |nested, _| {
            if let Type::TypeVar(v) = nested
                && outer.contains(db, v.identity(db))
            {
                repeated |= !variables.insert(v.identity(db));
            }
        });
        if repeated {
            Feedback::Repeated
        } else if !variables.is_empty()
            // An empty operand leaves a container's element type open. A returned callable
            // can still acquire parameter types from later passes over its sibling callbacks.
            && matches!(actual, Type::NominalInstance(_))
            && self.has_empty_operand
            && any_over_type_expanding_aliases(db, env, actual, |t| t.is_unknown())
        {
            Feedback::Empty
        } else {
            Feedback::None
        }
    }
}

impl<'db> Binding<'db> {
    #[expect(clippy::too_many_arguments)]
    pub(crate) fn joint_argument_contexts<'c>(
        &self,
        db: &'db dyn Db,
        env: &'c ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        binding: &CallableBinding<'db>,
        arguments: &CallArguments<'_, 'db>,
        initial: &CallArguments<'_, 'db>,
        tcx: TypeContext<'db>,
    ) -> Option<Vec<Option<Type<'db>>>> {
        let outer = self.signature.generic_context?;
        if outer
            .variables(db)
            .any(|v| v.is_paramspec(db) || v.is_typevartuple(db))
        {
            return None;
        }
        let current = arguments.with_self(binding.bound_type);
        let relations: Vec<_> = matched_argument_relations(
            self.signature.parameters(),
            &self.argument_matches,
            &current,
        )
        .map(|rel| {
            let original = rel.adjusted_argument_index.and_then(|i| {
                // An unpack can match several different values to parameters. Its whole source
                // expression cannot be opened as the producer of any one of those values.
                if arguments.is_variadic(i) {
                    return None;
                }
                let parameter =
                    self.signature.parameters()[rel.matched_parameter.index].annotated_type();
                let actual = initial.get(i)?.matched_type(parameter, None)?;
                let source = initial
                    .source_types(i)?
                    .expression_constraints(parameter, actual)?;
                let feedback = source.feedback(db, env, outer, rel.declared_type, actual);
                Some((i, source, feedback))
            });
            (rel, original)
        })
        .collect();
        let feedback = relations
            .iter()
            .filter_map(|(_, original)| original.map(|(_, _, risk)| risk))
            .max()?;
        if feedback == Feedback::None {
            return None;
        }
        let mut independent = ConstraintSet::from_bool(constraints, true);
        if let Some(result) = tcx.declared_type() {
            independent = self
                .unspecialized_return_type(db)
                .when_constraint_set_assignable_to_with_provenance(
                    db,
                    env,
                    result,
                    constraints,
                    if tcx.is_validity() {
                        ConstraintProvenance::VALIDITY
                    } else {
                        ConstraintProvenance::DECLARED
                    },
                );
        }
        let mut open = ConstraintSet::from_bool(constraints, true);
        let mut producer_preferences = Vec::new();
        let mut shapes = Vec::new();
        for (rel, original) in relations {
            if let Some((i, source, risk)) = original
                && (risk != Feedback::None || feedback == Feedback::Repeated)
                // A closed conditional can still determine the intrinsic depth in T | list[T].
                && (!source.locals.is_empty() || (risk == Feedback::Repeated
                    && source.closed.class_specialization(db, env).is_some()
                    && any_over_type_expanding_aliases(db, env, rel.declared_type, |ty| {
                        matches!(ty, Type::Union(_) | Type::Intersection(_))
                    })))
            {
                producer_preferences.extend_from_slice(&source.locals);
                if producer_preferences.len() > MAX_LOCAL_TYPEVARS {
                    return None;
                }
                shapes.push((i, source.ty, rel.declared_type));
                open.intersect(
                    db,
                    constraints,
                    source.relate(db, env, constraints, rel.declared_type),
                );
                continue;
            }
            independent = independent.and(db, constraints, || {
                rel.argument_type.when_constraint_set_assignable_to(
                    db,
                    env,
                    rel.declared_type,
                    constraints,
                )
            });
        }
        if shapes.is_empty() {
            return None;
        }
        let joint = GenericContext::from_typevar_instances(
            db,
            env,
            outer
                .variables(db)
                .chain(producer_preferences.iter().map(|&(v, _)| v)),
        );
        let solver = ProducerSolver {
            db,
            env,
            constraints,
            outer,
            joint,
        };
        let mut graph = open.and(db, constraints, || independent);
        let mut contexts = vec![None; arguments.len()];

        // A sibling can fix K without fixing V. Apply its context to list[K] immediately,
        // but keep V open when inferring a second parameter of type V | list[V].
        if let Some((chosen, fixed)) = solver.prefer(independent, graph, outer, |_| None) {
            graph = fixed;
            shapes.retain(|&(index, _, formal)| {
                let expected = formal.apply_specialization(db, chosen);
                if solver.contains_inferable(expected) {
                    return true;
                }
                contexts[index] = Some(expected);
                false
            });
        }
        if !shapes.is_empty()
            && let Some(chosen) = solver.specialize_producers(graph, &producer_preferences)
        {
            for (index, shape, _) in shapes {
                contexts[index] = Some(shape.apply_specialization(db, chosen));
            }
        }
        Some(contexts)
    }
}

struct ProducerSolver<'a, 'db, 'c> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    constraints: &'c ConstraintSetBuilder<'db>,
    outer: GenericContext<'db>,
    joint: GenericContext<'db>,
}

impl<'a: 'c, 'db, 'c> ProducerSolver<'a, 'db, 'c> {
    fn contains_inferable(&self, ty: Type<'db>) -> bool {
        any_over_type_expanding_aliases(self.db, self.env, ty, |nested| {
            nested
                .as_typevar()
                .is_some_and(|v| v.is_inferable(self.db, self.joint.inferable_typevars(self.db)))
        })
    }

    /// A preference can select a branch but cannot override constraints from an argument on
    /// another branch. Test the complete assignment against the original graph.
    fn with_assignment(
        &self,
        graph: ConstraintSet<'db, 'c>,
        assignment: impl IntoIterator<Item = (BoundTypeVarInstance<'db>, Type<'db>)>,
    ) -> Option<ConstraintSet<'db, 'c>> {
        let db = self.db;
        let env = self.env;
        let fixed = assignment.into_iter().fold(graph, |graph, (v, ty)| {
            graph.and(db, self.constraints, || {
                ConstraintSet::constrain_typevar_equivalence_bound(
                    db,
                    env,
                    self.constraints,
                    ConstraintProvenance::INFERRED,
                    v,
                    ty,
                )
            })
        });
        let feasible = matches!(
            fixed
                .solutions(db, env, self.joint.inferable_typevars(db))
                .ok()?,
            Solutions::Unconstrained | Solutions::Constrained(SolutionPaths::Complete(_))
        );
        feasible.then_some(fixed)
    }

    /// Check the merged proposal first so that alternatives for `T` in `T | list[T]`
    /// can contribute to a single `list[T]` context. Try individual solutions if that
    /// merge, or promoting literals in it, conflicts with another argument.
    fn prefer(
        &self,
        facts: ConstraintSet<'db, 'c>,
        graph: ConstraintSet<'db, 'c>,
        context: GenericContext<'db>,
        fallback: impl Fn(BoundTypeVarInstance<'db>) -> Option<Type<'db>>,
    ) -> Option<(Specialization<'db>, ConstraintSet<'db, 'c>)> {
        let db = self.db;
        let env = self.env;
        let mut builder = SpecializationBuilder::new(db, env, self.constraints, context);
        builder.record_constraint_set(facts);
        let inference = builder
            .build_inference_with(ReportDiagnostics::No, |_, _| None)
            .ok()?;
        inference.find_map_resolved_bindings(db, |candidate| {
            let bindings: Vec<_> = context
                .variables(db)
                .zip(candidate)
                .map(|(var, ty)| {
                    ty.and_then(|ty| {
                        if !self.contains_inferable(ty) {
                            return Some(ty);
                        }
                        // A bound such as int | str | E still supplies the concrete proposal
                        // int | str. The full graph below decides whether E can admit it.
                        let union = ty.as_union()?;
                        let closed = union.filter(db, |part| !self.contains_inferable(*part));
                        (!closed.is_never()).then_some(closed)
                    })
                    .or_else(|| fallback(var))
                })
                .collect();
            let promoted = bindings
                .iter()
                .map(|ty| ty.map(|ty| ty.promote(db, env)))
                .collect();
            [promoted, bindings].into_iter().find_map(|values| {
                let pairs = context
                    .variables(db)
                    .zip(&values)
                    .filter_map(|(var, ty)| Some((var, (*ty)?)));
                pairs.clone().next()?;
                let graph = self.with_assignment(graph, pairs)?;
                let values = context
                    .variables(db)
                    .zip(values)
                    .map(|(var, ty)| Some(ty.unwrap_or(Type::TypeVar(var))));
                Some((context.specialize_recursive(db, values), graph))
            })
        })
    }

    fn specialize_producers(
        &self,
        mut graph: ConstraintSet<'db, 'c>,
        locals: &[(BoundTypeVarInstance<'db>, Type<'db>)],
    ) -> Option<Specialization<'db>> {
        let db = self.db;
        if locals.is_empty() {
            return Some(self.joint.identity_specialization(db));
        }
        let env = self.env;
        // Prefer the independently inferred type of each constructed argument when the other
        // arguments permit it. This keeps [1] at list[int] in as_list([1]), while allowing a
        // second list to widen when the same type variable occurs in both parameters.
        for &(v, ty) in locals {
            if ty.is_fully_static(db, env)
                && let Some(fixed) = self.with_assignment(graph, [(v, ty)])
            {
                graph = fixed;
            }
        }
        if let Some((_, fixed)) = self.prefer(graph, graph, self.outer, |_| None) {
            graph = fixed;
        }
        let default = |v| locals.iter().find(|&&(w, _)| w == v).map(|&(_, ty)| ty);
        self.prefer(graph, graph, self.joint, default)
            .map(|(specialization, _)| specialization)
    }
}
