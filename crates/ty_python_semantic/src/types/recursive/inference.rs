//! Query-owned recursive equations. References identify their defining queries without
//! embedding the provisional results of those queries.
//!
//! # Approximation boundaries
//!
//! Equation collection follows query references and their deferred operations. It stops at
//! 32 distinct query/operation pairs. Equations containing `Dynamic` or `Divergent` are
//! approximated rather than solved. This traversal does not force lazy alias or member definitions.
//! These are termination limits, not tests for whether a recursive type has a solution.
//!
//! A solver result is used only when its root is resolved within the type budget and
//! contains no inference references. Otherwise,
//! unresolved backedges are replaced with `Divergent`, retaining outer constructors where
//! possible. A solving query that still cycles after `TAINTED_CYCLES` iterations falls back
//! to a single `Divergent` marker.
//!
//! Promotion can retain references as deferred operations.
//! Other semantic type mappings, including specialization and materialization, currently
//! apply to a finite approximation that retains constructors and cuts backedges with `Divergent`.
//! Structural substitutions handle references directly according to the requested substitution,
//! without this semantic fallback. These restrictions concern query-owned inference references; named
//! recursive aliases and closed structural solutions have their own mapping semantics.

use std::cell::{Cell, RefCell};

use ruff_python_ast::name::Name;
use salsa::plumbing::AsId;
use ty_python_core::definition::Definition;

use super::operations::RecursiveOperations;
use super::{RecursiveMapping, RecursiveOrigin, RecursiveSubstitution, RecursiveType};
use crate::types::class::ImplicitAttributeName;
use crate::types::constraints::TypeVarSolution;
use crate::types::constraints::resolution::SolutionType;
use crate::types::generics::walk_specialization_types;
use crate::types::infer::{InferExpression, infer_definition_types, infer_expression_types_impl};
use crate::types::visitor::{TypeCollector, TypeKind, TypeVisitor, walk_non_atomic_type};
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, DivergentType, DynamicType, MemberInference,
    Type, TypeContext, TypeMapping, TypeVarVariance, any_over_type,
};
use crate::{Db, FxIndexMap, Program, ProgramEnvironment, TAINTED_CYCLES};

/// Identifies the inference result that supplies an equation's body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub struct InferenceSource<'db>(pub(in crate::types) InferenceQuery<'db>);

/// A query equation after applying a sequence of deferred operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub struct InferenceKey<'db> {
    pub(super) source: InferenceSource<'db>,
    pub(super) operations: Option<RecursiveOperations<'db>>,
}

/// Inference results that can supply the body of a constructor equation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub(in crate::types) enum InferenceQuery<'db> {
    Binding(Definition<'db>),
    Expression(InferExpression<'db>),
    Attribute(ImplicitAttributeName<'db>),
    Member(MemberInference<'db>),
}

impl get_size2::GetSize for InferenceKey<'_> {}
impl get_size2::GetSize for InferenceSource<'_> {}

impl<'db> InferenceQuery<'db> {
    /// Return an acyclic value directly or a closed reference to its defining query.
    pub(in crate::types) fn value(self, db: &'db dyn Db, body: Type<'db>) -> Type<'db> {
        InferenceKey {
            source: InferenceSource(self),
            operations: None,
        }
        .value(db, body)
    }
}

/// A closed solution for display and its unfolding with query-owned recursive backedges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(in crate::types) struct InferenceSolution<'db> {
    /// The closed solver output, used without inserting it into a defining equation.
    pub(in crate::types) ty: Type<'db>,
    /// Query references prevent successive unfoldings from embedding previous solutions.
    pub(in crate::types) unfolded: Type<'db>,
}

impl<'db> InferenceSolution<'db> {
    /// Use the cycle approximation directly for display and type operations.
    fn approximate(ty: Type<'db>) -> Self {
        Self { ty, unfolded: ty }
    }
}

impl<'db> InferenceSource<'db> {
    pub(super) fn environment(self, db: &'db dyn Db) -> ProgramEnvironment<'db> {
        match self.0 {
            InferenceQuery::Binding(definition) => ProgramEnvironment::from_definition(definition),
            InferenceQuery::Expression(input) => {
                ProgramEnvironment::from_scope(input.into_inner(db).0.scope(db))
            }
            InferenceQuery::Attribute(attribute) => attribute.environment(db),
            InferenceQuery::Member(member) => member.environment(db),
        }
    }

    fn equation(self, db: &'db dyn Db) -> Type<'db> {
        match self.0 {
            InferenceQuery::Binding(definition) => {
                infer_definition_types(db, definition).raw_binding_type(definition)
            }
            InferenceQuery::Expression(input) => infer_expression_types_impl(db, input)
                .raw_expression_type(input.into_inner(db).0.node_ref(db)),
            InferenceQuery::Attribute(attribute) => attribute.equation(db),
            InferenceQuery::Member(member) => member.equation(db),
        }
    }
}

impl<'db> InferenceKey<'db> {
    fn environment(self, db: &'db dyn Db) -> ProgramEnvironment<'db> {
        self.source.environment(db)
    }

    fn reference(self, db: &'db dyn Db) -> Type<'db> {
        Type::Recursive(RecursiveType::inference(db, self))
    }

    /// Acyclic values remain direct. Recursive reads retain the identity of their defining query.
    fn value(self, db: &'db dyn Db, body: Type<'db>) -> Type<'db> {
        let env = self.environment(db);
        if !RecursiveInputs::contains(db, &env, [body])
            || matches!(body, Type::Recursive(recursive) if recursive.inference_key(db).is_some())
        {
            body
        } else {
            self.reference(db)
        }
    }

    fn equation(self, db: &'db dyn Db) -> Type<'db> {
        let body = self.source.equation(db);
        self.operations.map_or(body, |operations| {
            operations.apply(db, &self.environment(db), body)
        })
    }

    fn cycle_marker(self) -> Type<'db> {
        let id = match self.source.0 {
            InferenceQuery::Binding(definition) => definition.as_id(),
            InferenceQuery::Expression(input) => input.as_id(),
            InferenceQuery::Attribute(attribute) => attribute.as_id(),
            InferenceQuery::Member(member) => member.as_id(),
        };
        Type::Divergent(DivergentType::from_inference(id))
    }

    pub(in crate::types) fn solution(self, db: &'db dyn Db) -> InferenceSolution<'db> {
        inference_solution(db, self.environment(db).program(db), self)
    }

    /// Approximate query-owned backedges without changing independent recursive types.
    pub(super) fn approximate(self, db: &'db dyn Db) -> Type<'db> {
        // The unfolding retains query identities; a closed solution no longer distinguishes
        // these backedges from independently recursive components.
        let (equations, _) = self.equations(db, self.solution(db).unfolded);
        RecursiveMapping::approximate_inference(
            db,
            &self.environment(db),
            self.reference(db),
            self.cycle_marker(),
            &equations,
        )
    }

    /// Retain the equation's outer constructors while approximating unresolved backedges.
    fn approximate_equation(
        self,
        db: &'db dyn Db,
        equations: &FxIndexMap<InferenceKey<'db>, Type<'db>>,
    ) -> InferenceSolution<'db> {
        let env = self.environment(db);
        let divergent = self.cycle_marker();
        let ty = RecursiveMapping::approximate_inference(
            db,
            &env,
            self.reference(db),
            divergent,
            equations,
        )
        .recursive_type_normalized_impl(db, &env, divergent, false)
        .unwrap_or(divergent);
        InferenceSolution::approximate(ty)
    }

    /// Collect dependencies of the supplied root body and report whether all were found.
    fn equations(
        self,
        db: &'db dyn Db,
        root: Type<'db>,
    ) -> (FxIndexMap<InferenceKey<'db>, Type<'db>>, bool) {
        let env = self.environment(db);
        let mut equations = FxIndexMap::from_iter([(self, root)]);
        let mut cursor = 0;
        while let Some((_, body)) = equations.get_index(cursor) {
            let inputs = RecursiveInputs::collect(db, &env, [*body]);
            cursor += 1;
            for key in inputs {
                if !equations.contains_key(&key) {
                    // Query inputs and deferred operation sequences can grow. Distinct
                    // sequences are distinct equations, so this also bounds repeated
                    // projections that adjacent idempotence cannot simplify.
                    if equations.len() >= 32 {
                        return (equations, false);
                    }
                    equations.insert(key, key.equation(db));
                }
            }
        }
        (equations, true)
    }

    fn solve(self, db: &'db dyn Db) -> InferenceSolution<'db> {
        let env = self.environment(db);
        let root = self.equation(db);
        let (equations, complete) = self.equations(db, root);
        // Gradual equations still need a bound on materialization, but approximation
        // follows their dependencies to retain the known constructors.
        if !complete
            || equations.values().any(|body| {
                any_over_type(db, &env, *body, false, |ty| {
                    matches!(ty, Type::Dynamic(_) | Type::Divergent(_))
                })
            })
        {
            return self.approximate_equation(db, &equations);
        }
        let variables: Vec<_> = (0..equations.len())
            .map(|index| {
                BoundTypeVarInstance::synthetic(
                    db,
                    &env,
                    Name::new(format!("@inference_{index}")),
                    TypeVarVariance::Invariant,
                )
            })
            .collect();
        let replacements: Vec<_> = equations
            .keys()
            .zip(&variables)
            .map(|(key, variable)| (key.reference(db), Type::TypeVar(*variable)))
            .collect();
        let mapping = TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Replace(
            &replacements,
        )));
        let visitor = ApplyTypeMappingVisitor::new(&env);
        let mut symbolic = Vec::with_capacity(equations.len());
        for (body, variable) in equations.values().zip(&variables) {
            let body = body.apply_type_mapping_impl(db, &mapping, TypeContext::default(), &visitor);
            symbolic.push(TypeVarSolution {
                bound_typevar: *variable,
                solution: body,
            });
        }
        let result = match &TypeVarSolution::solve_equations(db, &env, &symbolic) {
            Ok(solution) if let Some(SolutionType::Resolved(ty)) = solution.first() => Some(*ty),
            _ => None,
        };
        let result = result.filter(|ty| !RecursiveInputs::contains(db, &env, [*ty]));
        let Some(ty) = result else {
            return self.approximate_equation(db, &equations);
        };
        // Retain query-owned backedges instead of embedding the closed solution in its equation.
        InferenceSolution { ty, unfolded: root }
    }
}

/// Solving may infer signatures and members, so it runs in an ordinary query.
#[salsa::tracked(
    returns(copy),
    cycle_initial=|_, id, _, _| InferenceSolution::approximate(Type::divergent(id)),
    cycle_fn=|_, cycle: &salsa::Cycle, _, current, _, _| if cycle.iteration() <= TAINTED_CYCLES { current } else { InferenceSolution::approximate(Type::divergent(cycle.id())) },
    heap_size=ruff_memory_usage::heap_size,
)]
fn inference_solution<'db>(
    db: &'db dyn Db,
    _program: Program<'db>,
    key: InferenceKey<'db>,
) -> InferenceSolution<'db> {
    key.solve(db)
}

/// Dependencies of an ambiguous operation. These are query references, not semantic type children.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveInputs<'db> {
    #[returns(ref)]
    keys: Box<[InferenceKey<'db>]>,
}

impl get_size2::GetSize for RecursiveInputs<'_> {}

impl<'db> RecursiveInputs<'db> {
    /// Merge equal gradual types without discarding either operation's dependencies.
    pub(in crate::types) fn merge(
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Option<Type<'db>> {
        let (Type::Dynamic(left), Type::Dynamic(right)) = (left, right) else {
            return None;
        };
        let inputs: Vec<_> = [left, right]
            .into_iter()
            .filter_map(|dynamic| match dynamic {
                DynamicType::AmbiguousOverload(inputs) => inputs,
                _ => None,
            })
            .collect();
        if inputs.is_empty() {
            return None;
        }
        Some(Self::unknown(
            db,
            inputs
                .into_iter()
                .flat_map(|inputs| inputs.keys(db).iter().copied()),
        ))
    }

    /// Inspect stored type structure, including unresolved cycle seeds, without solving it.
    pub(in crate::types) fn contains(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> bool {
        let visitor = InputVisitor::new(env);
        for ty in types {
            visitor.visit_type(db, ty);
        }
        visitor.initial.get() || !visitor.keys.borrow().is_empty()
    }

    /// Preserve the inputs of an ambiguous overload in a canonical dependency set.
    pub(in crate::types) fn unknown(
        db: &'db dyn Db,
        inputs: impl IntoIterator<Item = InferenceKey<'db>>,
    ) -> Type<'db> {
        let mut keys: Vec<_> = inputs.into_iter().collect();
        keys.sort_unstable_by_key(|key| RecursiveType::inference(db, *key).as_id());
        keys.dedup();
        Type::Dynamic(DynamicType::AmbiguousOverload(
            (!keys.is_empty()).then(|| Self::new(db, keys.into_boxed_slice())),
        ))
    }

    /// Collect query references without traversing their defining equations.
    pub(in crate::types) fn collect(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> Vec<InferenceKey<'db>> {
        let visitor = InputVisitor::new(env);
        for ty in types {
            visitor.visit_type(db, ty);
        }
        visitor.keys.into_inner()
    }
}

struct InputVisitor<'env, 'db> {
    env: &'env ProgramEnvironment<'db>,
    initial: Cell<bool>,
    keys: RefCell<Vec<InferenceKey<'db>>>,
    seen: TypeCollector<'db>,
}

impl<'env, 'db> InputVisitor<'env, 'db> {
    fn new(env: &'env ProgramEnvironment<'db>) -> Self {
        Self {
            env,
            initial: Cell::new(false),
            keys: RefCell::new(Vec::new()),
            seen: TypeCollector::default(),
        }
    }
}

impl<'db> TypeVisitor<'db> for InputVisitor<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }
    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }
    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        if self.seen.type_was_already_seen(ty) {
            return;
        }
        match ty {
            Type::Dynamic(DynamicType::AmbiguousOverload(Some(inputs))) => self
                .keys
                .borrow_mut()
                .extend(inputs.keys(db).iter().copied()),
            Type::RecursiveVar(_) => {}
            _ => {
                if let TypeKind::NonAtomic(node) = TypeKind::from(ty) {
                    walk_non_atomic_type(db, node, self);
                }
            }
        }
    }
    fn visit_recursive_type(&self, db: &'db dyn Db, recursive: RecursiveType<'db>) {
        if let Some(key) = recursive.inference_key(db) {
            self.keys.borrow_mut().push(key);
        } else if matches!(recursive.origin(db), RecursiveOrigin::InferenceCycle { .. }) {
            self.initial.set(true);
        } else {
            if let Some(arguments) = recursive.arguments(db) {
                walk_specialization_types(db, arguments, self);
            }
            // Inspect stored syntax without unfolding aliases or requesting inference in recovery.
            for body in recursive.graph(db).bodies(db) {
                self.visit_type(db, *body);
            }
        }
    }
}
