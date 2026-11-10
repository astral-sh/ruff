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

use std::rc::Rc;

use ruff_python_ast as ast;
use rustc_hash::{FxHashMap, FxHashSet};
use ty_python_core::ExpressionNodeKey;

use super::{Binding, CallableBinding, ProducerSignature, matched_argument_relations};
use crate::db::Db;
use crate::types::call::CallArguments;
use crate::types::constraints::{
    ConstraintProvenance, ConstraintSet, ConstraintSetBuilder, Solution, SolutionPaths, Solutions,
};
use crate::types::function::KnownFunction;
use crate::types::generics::{GenericContext, Specialization};
use crate::types::tuple::TupleSpec;
use crate::types::typevar::{BindingContext, TypeVarNonceGenerator, TypeVarSet};
use crate::types::visitor::any_over_type_expanding_aliases;
use crate::types::{BoundTypeVarInstance, ProgramEnvironment, Type, TypeContext, UnionType};

// Opening arguments is optional. Bound the size of the graph we construct before the solver's
// own traversal budgets take effect.
const MAX_PRODUCER_DEPTH: usize = 8;
pub(super) const MAX_LOCAL_TYPEVARS: usize = 16;
pub(super) const MAX_PRODUCER_RELATIONS: usize = 32;

/// Types inferred on the first argument-inference pass. Subsequent passes can refine these
/// types, but must not add the refinements to these sources of evidence.
#[derive(Clone, Copy)]
pub(crate) struct InitialArgumentTypes<'a, 'call, 'db> {
    pub(crate) arguments: &'a CallArguments<'call, 'db>,
    pub(crate) expressions: &'a [&'a ast::Expr],
    pub(crate) observed: &'a FxHashMap<ExpressionNodeKey, Type<'db>>,
    pub(crate) producers: &'a FxHashMap<ExpressionNodeKey, Rc<ProducerSignature<'db>>>,
}

struct ArgumentProducer<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    observed: &'a FxHashMap<ExpressionNodeKey, Type<'db>>,
    producers: &'a FxHashMap<ExpressionNodeKey, Rc<ProducerSignature<'db>>>,
    generator: &'a TypeVarNonceGenerator<'db>,
    locals: Vec<(BoundTypeVarInstance<'db>, Type<'db>)>,
    relations: Vec<(Type<'db>, Type<'db>)>,
}

impl<'db> ArgumentProducer<'_, 'db> {
    fn open(
        &mut self,
        expression: &ast::Expr,
        actual: Type<'db>,
        depth: usize,
    ) -> Option<Type<'db>> {
        let db = self.db;
        let env = self.env;
        if depth >= MAX_PRODUCER_DEPTH
            || self.locals.len() >= MAX_LOCAL_TYPEVARS
            || self.relations.len() >= MAX_PRODUCER_RELATIONS
        {
            return None;
        }
        if let ast::Expr::Tuple(tuple) = expression {
            if tuple.elts.iter().any(ast::Expr::is_starred_expr) {
                return None;
            }
            let spec = actual.exact_tuple_instance_spec(db)?;
            let TupleSpec::Fixed(fixed) = &*spec else {
                return None;
            };
            if tuple.elts.len() != fixed.len() {
                return None;
            }
            let elements = tuple
                .elts
                .iter()
                .zip(fixed.iter_all_elements())
                .map(|(elt, ty)| self.open(elt, ty, depth + 1))
                .collect::<Option<Vec<_>>>()?;
            return Some(Type::heterogeneous_tuple(db, env, elements));
        }

        match expression {
            ast::Expr::Call(call) => self.open_call(call, actual, depth),
            ast::Expr::List(_) | ast::Expr::Set(_) | ast::Expr::Dict(_) => {
                self.open_collection(expression, actual, depth)
            }
            _ => actual.is_fully_static(db, env).then_some(actual),
        }
    }

    fn open_call(
        &mut self,
        call: &ast::ExprCall,
        actual: Type<'db>,
        depth: usize,
    ) -> Option<Type<'db>> {
        let db = self.db;
        let env = self.env;
        let observed = self.observed;
        let child = |expr: &ast::Expr| observed.get(&expr.into()).copied();
        let callee = child(&call.func)?;
        if let Type::FunctionLiteral(func) = callee
            && func.is_known(db, KnownFunction::RevealType)
            && let [value] = &*call.arguments.args
        {
            return self.open(value, child(value)?, depth + 1);
        }
        let Some(producer) = self
            .producers
            .get(&call.into())
            .filter(|_| observed.get(&call.into()) == Some(&actual))
        else {
            return actual.is_fully_static(db, env).then_some(actual);
        };
        let context = producer.generic_context;
        let result = producer.return_ty;
        let mut fresh_types = Vec::new();
        let intrinsic = producer.inference.merged_specialization(db);
        for v in context.variables(db) {
            let fresh = BoundTypeVarInstance::new(
                db,
                v.typevar(db),
                BindingContext::Synthetic(env.program(db)),
                None,
                self.generator.next(),
            );
            self.locals
                .push((fresh, intrinsic.get(db, v).unwrap_or_else(Type::unknown)));
            fresh_types.push(Type::TypeVar(fresh));
        }
        let renaming = context.specialize(db, fresh_types);
        for relation in &producer.relations {
            let actual = relation.actual;
            let formal = relation.formal.apply_specialization(db, renaming);
            let Some(i) = relation.source_index else {
                // A synthetic constructor receiver may itself refer to the signature's locals,
                // as in the identity relation list[T] <= list[T] for list().
                self.relations
                    .push((actual.apply_specialization(db, renaming), formal));
                continue;
            };
            let operand = call.arguments.iter_source_order().nth(i)?;
            let src = operand.value();
            let lhs = if operand.is_variadic() {
                // The match already selected the individual element of an unpacked argument.
                actual.is_fully_static(db, env).then_some(actual)?
            } else if matches!(src, ast::Expr::Lambda(_)) {
                actual
            } else {
                self.open(src, actual, depth + 1)?
            };
            self.relations.push((lhs, formal));
        }
        Some(result.apply_specialization(db, renaming))
    }

    fn open_collection(
        &mut self,
        expression: &ast::Expr,
        actual: Type<'db>,
        depth: usize,
    ) -> Option<Type<'db>> {
        let db = self.db;
        let env = self.env;
        let observed = self.observed;
        let child = |expr: &ast::Expr| observed.get(&expr.into()).copied();
        let (class, spec) = actual.class_specialization(db, env)?;
        if !matches!(actual, Type::NominalInstance(_)) || spec.tuple(db).is_some() {
            return None;
        }
        let mut element_vars = Vec::new();
        for (v, intrinsic) in spec.generic_context(db).variables(db).zip(spec.types(db)) {
            if !v.typevar(db).is_simple(db) {
                return None;
            }
            let fresh = BoundTypeVarInstance::new(
                db,
                v.typevar(db),
                BindingContext::Synthetic(env.program(db)),
                None,
                self.generator.next(),
            );
            self.locals.push((fresh, *intrinsic));
            element_vars.push(Type::TypeVar(fresh));
        }
        let mut evidence = |expr: &ast::Expr, i| -> Option<()> {
            let value = child(expr)?;
            if matches!(value, Type::Dynamic(_)) {
                return None;
            }
            let shape = if expr.is_starred_expr() {
                value.iterate(db, env).homogeneous_element_type(db, env)
            } else {
                self.open(expr, value, depth + 1)?
            };
            self.relations.push((shape, *element_vars.get(i)?));
            Some(())
        };
        match expression {
            ast::Expr::List(list) => {
                for item in &list.elts {
                    evidence(item, 0)?;
                }
            }
            ast::Expr::Set(set) => {
                for item in &set.elts {
                    evidence(item, 0)?;
                }
            }
            ast::Expr::Dict(dict) => {
                for item in &dict.items {
                    evidence(item.key.as_ref()?, 0)?;
                    evidence(&item.value, 1)?;
                }
            }
            _ => return None,
        }
        Some(Type::instance(
            db,
            env,
            class.apply_specialization(db, |g| g.specialize(db, element_vars)),
        ))
    }
}

/// A single constructed value can supply conflicting contexts for its own type arguments.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Feedback {
    None,
    Empty,
    Repeated,
}

impl InitialArgumentTypes<'_, '_, '_> {
    /// An empty collection can leave an enclosing generic call's return type open, as in
    /// `identity([])`. Inspect only calls whose argument relations were captured on this pass.
    fn has_empty_operand(&self, expression: &ast::Expr, depth: usize) -> bool {
        if depth >= MAX_PRODUCER_DEPTH {
            return false;
        }
        match expression {
            ast::Expr::List(list) => list.elts.is_empty(),
            ast::Expr::Set(set) => set.elts.is_empty(),
            ast::Expr::Dict(dict) => dict.items.is_empty(),
            ast::Expr::Call(call) => {
                call.arguments.is_empty()
                    || (call.arguments.len() <= MAX_PRODUCER_RELATIONS
                        && self.producers.contains_key(&call.into())
                        && call.arguments.iter_source_order().any(|arg| {
                            !arg.is_variadic() && self.has_empty_operand(arg.value(), depth + 1)
                        }))
            }
            _ => false,
        }
    }

    fn feedback<'db>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        outer: GenericContext<'db>,
        parameter: Type<'db>,
        expression: &ast::Expr,
        actual: Type<'db>,
    ) -> Feedback {
        if let ast::Expr::Call(call) = expression
            && let Some(Type::FunctionLiteral(func)) = self.observed.get(&(&*call.func).into())
            && func.is_known(db, KnownFunction::RevealType)
            && let [value] = &*call.arguments.args
        {
            return self.feedback(db, env, outer, parameter, value, actual);
        }
        if let ast::Expr::Tuple(tuple) = expression
            && !tuple.elts.iter().any(ast::Expr::is_starred_expr)
            && let Some(formals) = parameter
                .resolve_type_alias(db)
                .exact_tuple_instance_spec(db)
            && let Some(actuals) = actual.exact_tuple_instance_spec(db)
            && let (Some(formals), Some(actuals)) =
                (formals.as_fixed_length(), actuals.as_fixed_length())
            && formals.len() == tuple.elts.len()
            && actuals.len() == tuple.elts.len()
        {
            // Each tuple element receives a separate expected type. Sharing a variable between
            // elements is how an ordinary call supplies context to its lambdas and literals.
            return tuple
                .elts
                .iter()
                .zip(formals.iter_all_elements())
                .zip(actuals.iter_all_elements())
                .map(|((elt, formal), actual)| self.feedback(db, env, outer, formal, elt, actual))
                .max()
                .unwrap_or(Feedback::None);
        }
        if matches!(
            expression,
            ast::Expr::Lambda(_)
                | ast::Expr::Name(_)
                | ast::Expr::Attribute(_)
                | ast::Expr::Subscript(_)
        ) {
            return Feedback::None;
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
            && self.has_empty_operand(expression, 0)
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
        env: &ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        binding: &CallableBinding<'db>,
        arguments: &CallArguments<'_, 'db>,
        initial: InitialArgumentTypes<'_, '_, 'db>,
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
                let expression = *initial.expressions.get(i)?;
                // An unpack can match several different values to parameters. Its whole source
                // expression cannot be opened as the producer of any one of those values.
                if arguments.is_variadic(i) {
                    return None;
                }
                let parameter =
                    self.signature.parameters()[rel.matched_parameter.index].annotated_type();
                let actual = initial.arguments.get(i)?.matched_type(parameter, None)?;
                let feedback =
                    initial.feedback(db, env, outer, rel.declared_type, expression, actual);
                Some((i, actual, feedback))
            });
            (rel, original)
        })
        .collect();
        if !relations
            .iter()
            .any(|(_, original)| original.is_some_and(|(_, _, risk)| risk != Feedback::None))
        {
            return None;
        }
        let repeated = relations
            .iter()
            .any(|(_, original)| original.is_some_and(|(_, _, risk)| risk == Feedback::Repeated));
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
        let generator = TypeVarNonceGenerator::default();
        let mut operand_relations = Vec::new();
        let mut fixed_relations = Vec::new();
        let mut producer_preferences = Vec::new();
        let mut shapes = vec![None; arguments.len()];
        for (rel, original) in relations {
            let flexible = original.and_then(|(i, actual, feedback)| {
                if feedback == Feedback::None && !repeated {
                    return None;
                }
                let mut producer = ArgumentProducer {
                    db,
                    env,
                    observed: initial.observed,
                    producers: initial.producers,
                    generator: &generator,
                    locals: Vec::new(),
                    relations: Vec::new(),
                };
                let shape = producer.open(initial.expressions[i], actual, 0);
                if shape.is_none() || producer.locals.is_empty() {
                    // A conditional or boolean expression may construct either of several
                    // values. When its parameter accepts both a value and a container of that
                    // value, its closed result still determines the intrinsic container depth.
                    if feedback == Feedback::Repeated
                        && actual.class_specialization(db, env).is_some()
                        && any_over_type_expanding_aliases(db, env, rel.declared_type, |ty| {
                            matches!(ty, Type::Union(_) | Type::Intersection(_))
                        })
                        && !matches!(
                            initial.expressions[i],
                            ast::Expr::Name(_) | ast::Expr::Attribute(_) | ast::Expr::Subscript(_)
                        )
                    {
                        shapes[i] = Some((actual, rel.declared_type));
                        return Some(actual);
                    }
                    return None;
                }
                let shape = shape?;
                operand_relations.extend(producer.relations);
                producer_preferences.extend(producer.locals);
                shapes[i] = Some((shape, rel.declared_type));
                Some(shape)
            });
            if let Some(shape) = flexible {
                operand_relations.push((shape, rel.declared_type));
                if producer_preferences.len() > MAX_LOCAL_TYPEVARS
                    || operand_relations.len() > MAX_PRODUCER_RELATIONS
                {
                    return None;
                }
            } else {
                fixed_relations.push((rel.argument_type, rel.declared_type));
            }
        }
        if shapes.iter().all(Option::is_none) {
            return None;
        }
        let constrain = |set: ConstraintSet<'db, 'c>, (actual, formal): (Type<'db>, Type<'db>)| {
            set.and(db, constraints, || {
                actual.when_constraint_set_assignable_to(db, env, formal, constraints)
            })
        };
        independent = fixed_relations.into_iter().fold(independent, constrain);
        let open = operand_relations
            .into_iter()
            .fold(ConstraintSet::from_bool(constraints, true), constrain);
        let fresh_vars: Vec<_> = producer_preferences.iter().map(|(v, _)| *v).collect();
        let variables =
            TypeVarSet::from_typevars(db, fresh_vars.iter().copied().chain(outer.variables(db)));
        let solver = ProducerSolver {
            db,
            env,
            constraints,
            outer,
            variables,
            has_declared_bounds: outer
                .variables(db)
                .chain(fresh_vars.iter().copied())
                .any(|v| v.typevar(db).bound_or_constraints(db, env).is_some()),
        };
        let mut graph = open.and(db, constraints, || independent);
        let mut contexts = vec![None; shapes.len()];

        // A sibling can fix K without fixing V. Apply its context to list[K] immediately,
        // but keep V open when inferring a second parameter of type V | list[V].
        if let Some((chosen, remaining, fixed)) = solver.prefer_outer(independent, graph) {
            graph = fixed;
            for (component, context) in shapes.iter_mut().zip(&mut contexts) {
                if let Some((_, formal)) = *component
                    && !any_over_type_expanding_aliases(db, env, formal, |ty| {
                        ty.as_typevar()
                            .is_some_and(|v| v.is_inferable(db, remaining))
                    })
                {
                    *context = Some(formal.apply_specialization(db, chosen));
                    *component = None;
                }
            }
            if shapes.iter().all(Option::is_none) {
                return Some(contexts);
            }
        }

        let producer_specialization = if producer_preferences.is_empty() {
            None
        } else {
            let Some(specialization) = solver.specialize_producers(graph, &producer_preferences)
            else {
                return Some(contexts);
            };
            Some(specialization)
        };
        for (component, context) in shapes.into_iter().zip(&mut contexts) {
            if let Some((shape, _)) = component {
                *context = Some(
                    producer_specialization.map_or(shape, |s| shape.apply_specialization(db, s)),
                );
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
    variables: TypeVarSet<'db>,
    has_declared_bounds: bool,
}

impl<'db, 'c> ProducerSolver<'_, 'db, 'c> {
    fn contains_inferable(&self, ty: Type<'db>) -> bool {
        any_over_type_expanding_aliases(self.db, self.env, ty, |nested| {
            nested
                .as_typevar()
                .is_some_and(|v| v.is_inferable(self.db, self.variables))
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
        let feasible = if self.has_declared_bounds {
            matches!(
                fixed.solutions(db, env, self.variables).ok()?,
                Solutions::Unconstrained | Solutions::Constrained(SolutionPaths::Complete(_))
            )
        } else {
            !fixed.is_never_satisfied_bounded(db, env, self.variables)?
        };
        feasible.then_some(fixed)
    }

    /// For `T | list[T]`, one caller can contribute both `int` and `list[int]`.
    /// Try their union for `T` before specializing enclosing parameters. Specializing
    /// `list[T]` separately would produce a union of invariant lists. If the combined
    /// assignment is inconsistent, retain the associations from a single solution path.
    fn prefer_outer(
        &self,
        facts: ConstraintSet<'db, 'c>,
        graph: ConstraintSet<'db, 'c>,
    ) -> Option<(Specialization<'db>, TypeVarSet<'db>, ConstraintSet<'db, 'c>)> {
        let db = self.db;
        let env = self.env;
        let Solutions::Constrained(SolutionPaths::Complete(paths)) = facts
            .solutions(db, env, self.outer.inferable_typevars(db))
            .ok()?
        else {
            return None;
        };
        let paths: Vec<Vec<_>> = paths
            .iter()
            .map(|path| {
                self.outer
                    .variables(db)
                    .map(|v| solution_for(path, v).map(|ty| ty.promote(db, env)))
                    .collect::<Vec<_>>()
            })
            .filter(|values| {
                !values
                    .iter()
                    .flatten()
                    .any(|&ty| self.contains_inferable(ty))
            })
            .collect();
        let first = paths.first()?;
        let merged = (0..first.len())
            .map(|i| {
                let types = paths.iter().map(|p| p[i]).collect::<Option<Vec<_>>>()?;
                Some(UnionType::from_elements(db, env, types))
            })
            .collect();
        for values in std::iter::once(merged).chain(paths) {
            if values.iter().all(Option::is_none) {
                continue;
            }
            let assignment = self
                .outer
                .variables(db)
                .zip(&values)
                .filter_map(|(v, t)| Some((v, (*t)?)));
            if let Some(fixed) = self.with_assignment(graph, assignment) {
                let unsolved = TypeVarSet::from_typevars(
                    db,
                    self.outer
                        .variables(db)
                        .zip(&values)
                        .filter_map(|(v, ty)| ty.is_none().then_some(v)),
                );
                return Some((self.outer.specialize_recursive(db, values), unsolved, fixed));
            }
        }
        None
    }

    fn specialize_producers(
        &self,
        mut graph: ConstraintSet<'db, 'c>,
        locals: &[(BoundTypeVarInstance<'db>, Type<'db>)],
    ) -> Option<Specialization<'db>> {
        let db = self.db;
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
        let local_context =
            GenericContext::from_typevar_instances(db, env, locals.iter().map(|&(v, _)| v));
        let local_vars = local_context.inferable_typevars(db);
        let outer_facts = graph.reduce_inferable_bounded(db, env, self.constraints, local_vars)?;
        if let Some((_, _, fixed)) = self.prefer_outer(outer_facts, graph) {
            graph = fixed;
        }

        let projected = graph.reduce_inferable_bounded(
            db,
            env,
            self.constraints,
            self.outer.inferable_typevars(db),
        )?;
        let paths = match projected.solutions(db, env, local_vars).ok()? {
            Solutions::Unconstrained => vec![None],
            Solutions::Constrained(SolutionPaths::Complete(paths)) => {
                paths.into_iter().map(Some).collect()
            }
            _ => return None,
        };

        // All producers must choose the same solution path. Picking values independently
        // can give two arguments incompatible types for the caller's unchanged type variable.
        for path in paths {
            let exact: Vec<_> = locals
                .iter()
                .map(|&(v, fallback)| {
                    path.as_ref()
                        .and_then(|p| solution_for(p, v))
                        .filter(|&ty| !self.contains_inferable(ty))
                        .unwrap_or(fallback)
                })
                .collect();
            let promoted = exact.iter().map(|t| t.promote(db, env)).collect::<Vec<_>>();
            for candidate in [promoted, exact] {
                if self
                    .with_assignment(
                        graph,
                        locals.iter().zip(&candidate).map(|(&(v, _), &ty)| (v, ty)),
                    )
                    .is_some()
                {
                    return Some(local_context.specialize(db, candidate));
                }
            }
        }
        None
    }
}

fn solution_for<'db>(
    path: &Solution<'db>,
    variable: BoundTypeVarInstance<'db>,
) -> Option<Type<'db>> {
    path.solved_typevars
        .iter()
        .find(|s| s.bound_typevar == variable)
        .map(|s| s.solution)
}
