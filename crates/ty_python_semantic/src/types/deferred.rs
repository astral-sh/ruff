//! Closed type expressions whose interpretation needs a recursive proof context.

use std::cell::{Cell, RefCell};

use rustc_hash::{FxHashMap, FxHashSet};

use super::projection::{ObservationEdge, ObservedType};
use super::recursive::RecursiveOperation;
use super::relation::RelationContext;
use super::set_theoretic::TypeNormalization;
use super::visitor::{TypeVisitor, any_over_type_expanding_aliases};
use super::{
    ApplyTypeMappingVisitor, BindingContext, BoundTypeVarIdentity, BoundTypeVarInstance,
    IntersectionBuilder, MaterializationKind, Type, TypeContext, TypeMapping,
    TypeVarBoundOrConstraints, UnionBuilder, VarianceInferable, VarianceTerm,
};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub enum DeferredArgumentMode {
    Restrict,
    Materialize(MaterializationKind, bool),
}

/// A type argument interpreted within its parameter's declared domain.
/// Later operations remain ordered after this interpretation, rather than being pushed into it.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct DeferredType<'db> {
    #[returns(copy)]
    pub(super) argument: Type<'db>,
    #[returns(copy)]
    pub(super) parameter: BoundTypeVarInstance<'db>,
    #[returns(copy)]
    pub(super) mode: DeferredArgumentMode,
    #[returns(ref)]
    pub(super) operations: Box<[RecursiveOperation<'db>]>,
}

impl get_size2::GetSize for DeferredType<'_> {}

impl<'db> DeferredType<'db> {
    pub fn environment(self, db: &'db dyn Db) -> ProgramEnvironment<'db> {
        let program = match self.parameter(db).binding_context(db) {
            BindingContext::Definition(definition) => definition.program(db),
            BindingContext::Synthetic(program) => program,
        };
        ProgramEnvironment::from_program(program)
    }

    pub(super) fn materialized_argument(
        db: &'db dyn Db,
        argument: Type<'db>,
        parameter: BoundTypeVarInstance<'db>,
        kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        let materialized = argument.materialize(db, kind, &visitor.for_type_construction());
        if materialized == argument {
            return argument;
        }
        if kind == MaterializationKind::Bottom || !parameter.typevar(db).has_declared_domain(db) {
            return materialized;
        }
        Type::Deferred(Self::new_internal(
            db,
            argument,
            parameter,
            DeferredArgumentMode::Materialize(
                kind,
                visitor.materialize_typevar_bounds_and_defaults,
            ),
            Box::default(),
        ))
    }

    pub(super) fn restricted_argument(
        db: &'db dyn Db,
        argument: Type<'db>,
        parameter: BoundTypeVarInstance<'db>,
    ) -> Type<'db> {
        let program = match parameter.binding_context(db) {
            BindingContext::Definition(definition) => definition.program(db),
            BindingContext::Synthetic(program) => program,
        };
        let env = ProgramEnvironment::from_program(program);
        if !parameter.typevar(db).has_declared_domain(db)
            || super::recursive::structurally_static(db, &env, argument)
        {
            return argument;
        }
        Type::Deferred(Self::new_internal(
            db,
            argument,
            parameter,
            DeferredArgumentMode::Restrict,
            Box::default(),
        ))
    }

    pub(super) fn constructor(self, db: &'db dyn Db) -> Self {
        Self::new_internal(
            db,
            Type::TypeVar(self.parameter(db)),
            self.parameter(db),
            self.mode(db),
            Box::default(),
        )
    }

    /// A completed materialization is fixed by subsequent materializations. Stored arguments
    /// may still contain `Any`; they describe the input expression, not the materialized value.
    pub(super) fn is_materialized_for(self, db: &'db dyn Db, map_bounds: bool) -> bool {
        let mut materialized = match self.mode(db) {
            DeferredArgumentMode::Materialize(_, bounds) => Some(bounds),
            DeferredArgumentMode::Restrict => None,
        };
        for operation in self.operations(db) {
            materialized = match operation {
                RecursiveOperation::Materialize(_, bounds) => Some(*bounds),
                _ => None,
            };
        }
        materialized.is_some_and(|bounds| bounds || !map_bounds)
    }

    /// Compare the finite inputs after replaying the expression's earlier operations.
    /// No declaration body or free-variable summary is needed to prove a mapping unchanged.
    fn inputs_change(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> bool {
        let visitor = visitor.for_type_construction();
        let argument = match self.mode(db) {
            DeferredArgumentMode::Restrict => self.argument(db),
            DeferredArgumentMode::Materialize(kind, map_bounds) => {
                let mut materialization = visitor.for_new_mapping();
                materialization.materialize_typevar_bounds_and_defaults = map_bounds;
                self.argument(db).materialize(db, kind, &materialization)
            }
        };
        let mut inputs = vec![argument];
        inputs.extend(self.domain(db, visitor.env));
        inputs.into_iter().any(|mut input| {
            for operation in self.operations(db) {
                operation.with_mapping(|mapping| {
                    let mut visitor = visitor.for_new_mapping();
                    if let RecursiveOperation::Materialize(_, bounds) = operation {
                        visitor.materialize_typevar_bounds_and_defaults = *bounds;
                    }
                    input = input.apply_type_mapping_impl(
                        db,
                        &mapping,
                        TypeContext::default(),
                        &visitor,
                    );
                });
            }
            input.apply_type_mapping_impl(db, mapping, TypeContext::default(), &visitor) != input
        })
    }

    /// Retrieve declaration syntax without materializing or comparing the domain.
    pub(super) fn domain(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        Some(
            match self
                .parameter(db)
                .typevar(db)
                .bound_or_constraints(db, env)?
            {
                TypeVarBoundOrConstraints::UpperBound(bound) => bound,
                TypeVarBoundOrConstraints::Constraints(constraints) => constraints
                    .elements(db)
                    .iter()
                    .copied()
                    .fold(
                        UnionBuilder::new(db, env).normalization(TypeNormalization::Structural),
                        UnionBuilder::add,
                    )
                    .build(),
            },
        )
    }

    /// Interpret the expression only after its observation obligation has been registered.
    pub(super) fn observe(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        operand: &ObservedType<'db>,
        context: &RelationContext<'db>,
    ) -> Option<ObservedType<'db>> {
        self.observe_with_domains(db, env, operand, context, &DomainCompletion::default())
            .ok()
    }

    fn observe_with_domains(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        operand: &ObservedType<'db>,
        context: &RelationContext<'db>,
        domains: &DomainCompletion<'db>,
    ) -> Result<ObservedType<'db>, DomainFailure<'db>> {
        // A repeated request with the same input has already reached its domain in this
        // computation. Later operations do not change that earlier dependency.
        if domains
            .active
            .borrow()
            .keys()
            .any(|key| key.parameter == self.parameter(db).identity(db))
            && let Some(domain) = self.domain(db, env)
        {
            let key = DomainKey {
                parameter: self.parameter(db).identity(db),
                upper: domain.materialize(
                    db,
                    MaterializationKind::Top,
                    &ApplyTypeMappingVisitor::new_for_type_construction(env),
                ),
            };
            if domains.active.borrow().get(&key).is_some_and(|active| {
                active.argument(db) == self.argument(db) && active.mode(db) == self.mode(db)
            }) {
                return Err(DomainFailure::Recursive(key));
            }
        }
        context
            .observe(db, operand, || {
                Some((|| {
                    let argument = operand
                        .project(db, env, ObservationEdge::DeferredArgument)
                        .ok_or(DomainFailure::Incomplete)?;
                    let visitor = ApplyTypeMappingVisitor::new_for_type_construction(env);
                    let (mut result, needs_domain) = match self.mode(db) {
                        DeferredArgumentMode::Restrict => {
                            let gradual =
                                any_over_type_expanding_aliases(db, env, argument.ty, |ty| {
                                    ty.is_dynamic()
                                });
                            (argument, gradual)
                        }
                        DeferredArgumentMode::Materialize(kind, map_bounds) => {
                            let mut materialization = visitor.for_new_mapping();
                            materialization.materialize_typevar_bounds_and_defaults = map_bounds;
                            let materialized = argument.apply_mapping(
                                db,
                                &TypeMapping::Materialize(kind),
                                &materialization,
                            );
                            let changed = if materialized.ty == argument.ty {
                                false
                            } else {
                                !context
                                    .try_equivalent_eager(db, env, argument, materialized.clone())
                                    .ok_or(DomainFailure::Incomplete)?
                            };
                            (materialized, changed)
                        }
                    };
                    if needs_domain {
                        match domains.upper_bound(db, env, self, operand, context) {
                            Ok(Some(upper)) => {
                                let intersection = IntersectionBuilder::new(db, env)
                                    .normalization(TypeNormalization::Structural)
                                    .positive_elements([result.ty, upper.ty])
                                    .build();
                                result =
                                    result.normalized(intersection, vec![result.clone(), upper]);
                            }
                            Ok(None) => {}
                            Err(error) => return Err(error),
                        }
                    }
                    for operation in self.operations(db) {
                        let mut visitor = visitor.for_new_mapping();
                        if let RecursiveOperation::Materialize(_, bounds) = operation {
                            visitor.materialize_typevar_bounds_and_defaults = *bounds;
                        }
                        result = operation
                            .with_mapping(|mapping| result.apply_mapping(db, &mapping, &visitor));
                    }
                    Ok(result)
                })())
            })
            .ok_or(DomainFailure::Incomplete)?
    }

    /// Observe a new independent expression, as used by a non-proof consumer such as display.
    pub(super) fn resolve(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        let context = RelationContext::default();
        ObservedType::root(Type::Deferred(self))
            .unfold_in_context(db, env, &context)
            .map_or(Type::Deferred(self), |observed| observed.ty)
    }

    /// Return an independent observation only when it exposes a different outer type.
    pub fn try_resolve(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<Type<'db>> {
        let resolved = self.resolve(db, env);
        (resolved != Type::Deferred(self)).then_some(resolved)
    }

    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        if matches!(mapping, TypeMapping::Normalize) {
            return Type::Deferred(self);
        }
        if matches!(mapping, TypeMapping::Materialize(_))
            && self.is_materialized_for(db, visitor.materialize_typevar_bounds_and_defaults)
        {
            return Type::Deferred(self);
        }
        if matches!(mapping, TypeMapping::ApplyRecursiveSubstitution(_)) {
            return Type::Deferred(Self::new_internal(
                db,
                self.argument(db).apply_type_mapping_impl(
                    db,
                    mapping,
                    tcx,
                    &visitor.for_type_construction(),
                ),
                self.parameter(db),
                self.mode(db),
                self.operations(db)
                    .iter()
                    .map(|operation| operation.map_types(db, mapping, visitor))
                    .collect::<Box<[_]>>(),
            ));
        }
        let Some(operation) =
            RecursiveOperation::capture(mapping, visitor.materialize_typevar_bounds_and_defaults)
        else {
            return Type::Deferred(self);
        };
        if operation.is_substitution() && !self.inputs_change(db, mapping, visitor) {
            return Type::Deferred(self);
        }
        if matches!(
            operation,
            RecursiveOperation::Materialize(..)
                | RecursiveOperation::Promote(..)
                | RecursiveOperation::ReplaceParameterDefaults
                | RecursiveOperation::EagerExpansion
        ) && self.operations(db).last() == Some(&operation)
        {
            return Type::Deferred(self);
        }
        let mut operations = self.operations(db).to_vec();
        operations.push(operation);
        Type::Deferred(Self::new_internal(
            db,
            self.argument(db),
            self.parameter(db),
            self.mode(db),
            operations.into_boxed_slice(),
        ))
    }

    pub(super) fn visit_types(self, db: &'db dyn Db, visitor: &(impl TypeVisitor<'db> + ?Sized)) {
        visitor.visit_type(db, self.argument(db));
        super::typevar::walk_type_var_domain(db, self.parameter(db).typevar(db), visitor);
        for operation in self.operations(db) {
            operation.visit_types(db, visitor);
        }
    }
}

impl<'db> DeferredType<'db> {
    pub(super) fn captured_types(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Vec<Type<'db>> {
        struct Captures<'a, 'db> {
            env: &'a ProgramEnvironment<'db>,
            types: RefCell<Vec<Type<'db>>>,
        }
        impl<'db> TypeVisitor<'db> for Captures<'_, 'db> {
            fn program_environment(&self) -> &ProgramEnvironment<'db> {
                self.env
            }
            fn should_visit_lazy_type_attributes(&self) -> bool {
                true
            }
            fn visit_type(&self, _db: &'db dyn Db, ty: Type<'db>) {
                self.types.borrow_mut().push(ty);
            }
        }
        let captures = Captures {
            env,
            types: RefCell::default(),
        };
        self.visit_types(db, &captures);
        captures.types.into_inner()
    }

    /// Collect variance from both possible domain-restriction results without choosing a branch.
    pub(super) fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        let mut alternatives = vec![self.argument(db)];
        if let Some(domain) = self.domain(db, env) {
            alternatives.push(
                IntersectionBuilder::new(db, env)
                    .normalization(TypeNormalization::Structural)
                    .positive_elements([self.argument(db), domain])
                    .build(),
            );
        }
        let visitor = ApplyTypeMappingVisitor::new_for_type_construction(env);
        for operation in self.operations(db) {
            for alternative in &mut alternatives {
                *alternative = operation.with_mapping(|mapping| {
                    let mapping = match mapping {
                        TypeMapping::ApplySpecializationWithMaterialization {
                            specialization,
                            ..
                        } => TypeMapping::ApplySpecialization(specialization),
                        TypeMapping::Materialize(_) | TypeMapping::EagerExpansion => {
                            return *alternative;
                        }
                        mapping => mapping,
                    };
                    alternative.apply_type_mapping_impl(
                        db,
                        &mapping,
                        TypeContext::default(),
                        &visitor,
                    )
                });
            }
        }
        VarianceTerm::join(
            db,
            alternatives
                .into_iter()
                .map(|ty| ty.variance_of(db, env, typevar)),
        )
    }
}

/// Completing a bound can require other parameter domains through stored type arguments.
/// A recursive dependency has no finite static upper bound. Propagate that failure to the
/// parameter where the cycle began; enclosing acyclic domains retain their own restrictions.
#[derive(Default)]
struct DomainCompletion<'db> {
    active: RefCell<FxHashMap<DomainKey<'db>, DeferredType<'db>>>,
}

/// Materializing bound metadata does not create a new parameter. Its canonical upper domain
/// does retain captured arguments, so different specializations of a captured bound stay distinct.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct DomainKey<'db> {
    parameter: BoundTypeVarIdentity<'db>,
    upper: Type<'db>,
}

#[derive(Clone, Copy)]
enum DomainFailure<'db> {
    Recursive(DomainKey<'db>),
    Incomplete,
}

impl<'db> DomainCompletion<'db> {
    fn upper_bound(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        deferred: DeferredType<'db>,
        operand: &ObservedType<'db>,
        context: &RelationContext<'db>,
    ) -> Result<Option<ObservedType<'db>>, DomainFailure<'db>> {
        let Some(domain) = operand.project(db, env, ObservationEdge::DeferredDomain) else {
            return Ok(None);
        };
        let mut upper = domain.apply_mapping(
            db,
            &TypeMapping::Materialize(MaterializationKind::Top),
            &ApplyTypeMappingVisitor::new_for_type_construction(env),
        );
        let key = DomainKey {
            parameter: deferred.parameter(db).identity(db),
            upper: upper.ty,
        };
        if self.active.borrow().contains_key(&key) {
            return Err(DomainFailure::Recursive(key));
        }
        self.active.borrow_mut().insert(key, deferred);
        let result = (|| {
            let mut exposed = FxHashSet::default();
            while matches!(upper.ty, Type::TypeAlias(_))
                || matches!(upper.ty, Type::Recursive(recursive) if recursive.is_alias(db))
            {
                if !exposed.insert(upper.ty.to_type_identity(db)) {
                    return Err(DomainFailure::Incomplete);
                }
                upper = upper
                    .unfold_in_context(db, env, context)
                    .ok_or(DomainFailure::Incomplete)?;
            }
            self.complete(db, env, upper.clone(), context)?;
            Ok(Some(upper))
        })();
        self.active.borrow_mut().remove(&key);
        match result {
            Err(DomainFailure::Recursive(owner)) if owner == key => Ok(None),
            result => result,
        }
    }

    fn complete(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        upper: ObservedType<'db>,
        context: &RelationContext<'db>,
    ) -> Result<(), DomainFailure<'db>> {
        let mut pending = vec![upper];
        let mut visited = FxHashSet::default();
        let mut unfolded_declarations = FxHashMap::default();
        while let Some(operand) = pending.pop() {
            if !visited.insert(operand.ty) {
                continue;
            }
            match operand.ty {
                Type::Deferred(deferred) => {
                    pending.push(deferred.observe_with_domains(db, env, &operand, context, self)?);
                }
                Type::TypeAlias(_) | Type::Recursive(_) => {
                    pending.extend(operand.stored_children(db, env));
                    let constructor = operand.ty.to_type_identity(db);
                    if let Some(previous) = unfolded_declarations.get(&constructor) {
                        if *previous != operand.ty
                            && declaration_may_request_domains(db, env, operand.ty)
                        {
                            return Err(DomainFailure::Incomplete);
                        }
                    } else {
                        unfolded_declarations.insert(constructor, operand.ty);
                        let unfolded = operand
                            .unfold_in_context(db, env, context)
                            .ok_or(DomainFailure::Incomplete)?;
                        pending.push(unfolded);
                    }
                }
                _ => pending.extend(operand.stored_children(db, env)),
            }
        }
        Ok(())
    }
}

/// Prove that a recursive declaration cannot introduce another parameter-domain request,
/// regardless of how its arguments grow. Ordinary recursive aliases remain closed symbolic
/// types; only bounded parameters or already-deferred operations can add domain dependencies.
fn declaration_may_request_domains<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> bool {
    struct Requests<'a, 'db> {
        env: &'a ProgramEnvironment<'db>,
        pending: RefCell<Vec<Type<'db>>>,
        may_request: Cell<bool>,
    }
    impl<'db> TypeVisitor<'db> for Requests<'_, 'db> {
        fn program_environment(&self) -> &ProgramEnvironment<'db> {
            self.env
        }
        fn should_visit_lazy_type_attributes(&self) -> bool {
            false
        }
        fn notify_skipped_lazy_type_attributes(&self) {
            self.may_request.set(true);
        }
        fn visit_type(&self, _db: &'db dyn Db, ty: Type<'db>) {
            self.pending.borrow_mut().push(ty);
        }
        fn visit_generic_alias_type(&self, db: &'db dyn Db, alias: super::GenericAlias<'db>) {
            let specialization = alias.specialization(db);
            if specialization
                .generic_context(db)
                .variables(db)
                .any(|parameter| parameter.typevar(db).has_declared_domain(db))
            {
                self.may_request.set(true);
            }
            super::generics::walk_specialization_types(db, specialization, self);
        }
        fn visit_bound_type_var_type(&self, db: &'db dyn Db, parameter: BoundTypeVarInstance<'db>) {
            self.may_request
                .set(self.may_request.get() || parameter.typevar(db).has_declared_domain(db));
        }
    }
    let requests = Requests {
        env,
        pending: RefCell::new(vec![ty]),
        may_request: Cell::new(false),
    };
    let mut seen = FxHashSet::default();
    let mut declarations = FxHashSet::default();
    while !requests.may_request.get() {
        let Some(ty) = requests.pending.borrow_mut().pop() else {
            break;
        };
        if !seen.insert(ty) {
            continue;
        }
        match ty {
            Type::Deferred(_) | Type::RecursiveVar(_) => return true,
            Type::TypeVar(parameter) => requests.visit_bound_type_var_type(db, parameter),
            Type::TypeAlias(alias) => {
                if alias.generic_context(db).is_some_and(|context| {
                    context
                        .variables(db)
                        .any(|parameter| parameter.typevar(db).has_declared_domain(db))
                }) {
                    return true;
                }
                alias.visit_application_types(db, &requests);
                if declarations.insert(alias.definition(db)) {
                    requests.visit_type(db, alias.raw_value_type(db));
                }
            }
            Type::Recursive(recursive) => {
                if !recursive.is_alias(db) {
                    return true;
                }
                if let Some(arguments) = recursive.arguments(db) {
                    if arguments
                        .generic_context(db)
                        .variables(db)
                        .any(|parameter| parameter.typevar(db).has_declared_domain(db))
                    {
                        return true;
                    }
                    super::generics::walk_specialization_types(db, arguments, &requests);
                }
                if declarations.insert(recursive.definition(db)) {
                    requests.visit_type(db, recursive.constructor(db).unfold(db, env).into_type());
                }
            }
            _ => {
                if let super::visitor::TypeKind::NonAtomic(ty) = ty.into() {
                    super::visitor::walk_non_atomic_type(db, ty, &requests);
                }
            }
        }
    }
    requests.may_request.get()
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use ruff_python_ast::name::Name;
    use ty_python_core::ProgramFile;

    use super::*;
    use crate::db::tests::setup_db;
    use crate::place::global_symbol;
    use crate::types::generics::ApplySpecialization;
    use crate::types::{ClassType, GenericAlias, KnownClass, TypeVarVariance};

    #[test]
    fn static_argument_is_not_intersected_with_an_invalid_domain() {
        let db = setup_db();
        let env = db.program_environment();
        let int = KnownClass::Int.to_instance(&db, &env);
        let str = KnownClass::Str.to_instance(&db, &env);
        let parameter = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        )
        .map_bound_or_constraints(&db, |_| Some(TypeVarBoundOrConstraints::UpperBound(int)));
        let expression = DeferredType::materialized_argument(
            &db,
            str,
            parameter,
            MaterializationKind::Top,
            &ApplyTypeMappingVisitor::new_for_type_construction(&env),
        );
        assert_eq!(expression, str);
    }

    #[test]
    fn substitution_stays_after_the_deferred_materialization() {
        let db = setup_db();
        let env = db.program_environment();
        let captured = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("U"),
            TypeVarVariance::Invariant,
        );
        let parameter = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        )
        .map_bound_or_constraints(&db, |_| {
            Some(TypeVarBoundOrConstraints::UpperBound(Type::TypeVar(
                captured,
            )))
        });
        let visitor = ApplyTypeMappingVisitor::new_for_type_construction(&env);
        let expression = DeferredType::materialized_argument(
            &db,
            Type::any(),
            parameter,
            MaterializationKind::Top,
            &visitor,
        );
        let substituted = expression.apply_type_mapping_impl(
            &db,
            &TypeMapping::ApplySpecialization(ApplySpecialization::Single(captured, Type::any())),
            TypeContext::default(),
            &visitor,
        );
        let Type::Deferred(substituted) = substituted else {
            panic!("substitution must preserve operation order");
        };
        assert_eq!(substituted.resolve(&db, &env), Type::any());

        let replaced_domain = parameter.map_bound_or_constraints(&db, |_| {
            Some(TypeVarBoundOrConstraints::UpperBound(Type::any()))
        });
        let expression = DeferredType::materialized_argument(
            &db,
            Type::any(),
            replaced_domain,
            MaterializationKind::Top,
            &visitor,
        );
        let Type::Deferred(materialized) = expression else {
            panic!("bounded argument must retain its interpretation");
        };
        assert_eq!(materialized.resolve(&db, &env), Type::object());
    }
    #[test]
    fn deferred_substitutions_reach_a_structural_fixed_point() {
        let db = setup_db();
        let env = db.program_environment();
        let captured = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("U"),
            TypeVarVariance::Invariant,
        );
        let unrelated = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("V"),
            TypeVarVariance::Invariant,
        );
        let parameter = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        )
        .map_bound_or_constraints(&db, |_| {
            Some(TypeVarBoundOrConstraints::UpperBound(Type::TypeVar(
                captured,
            )))
        });
        let visitor = ApplyTypeMappingVisitor::new_for_type_construction(&env);
        let expression = DeferredType::materialized_argument(
            &db,
            Type::any(),
            parameter,
            MaterializationKind::Top,
            &visitor,
        );
        let unrelated_mapping =
            TypeMapping::ApplySpecialization(ApplySpecialization::Single(unrelated, Type::any()));
        assert_eq!(
            expression.apply_type_mapping_impl(
                &db,
                &unrelated_mapping,
                TypeContext::default(),
                &visitor
            ),
            expression
        );

        let mapping =
            TypeMapping::ApplySpecialization(ApplySpecialization::Single(captured, Type::any()));
        let substituted =
            expression.apply_type_mapping_impl(&db, &mapping, TypeContext::default(), &visitor);
        assert_ne!(substituted, expression);
        assert_eq!(
            substituted.apply_type_mapping_impl(&db, &mapping, TypeContext::default(), &visitor),
            substituted
        );
        for kind in [MaterializationKind::Top, MaterializationKind::Bottom] {
            assert_eq!(expression.materialize(&db, kind, &visitor), expression);
        }
    }

    #[test]
    fn declaration_parameters_are_not_captures_of_a_specialized_bound() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
from typing import Any
class Recursive[T: "Recursive[Any]"]: ...
domain: Recursive[Any]
def f(value: Any) -> Any: return value
"#,
        )
        .unwrap();
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/a.py").unwrap();
        let file = ProgramFile::new(&db, file, env.program(&db));
        let domain = global_symbol(&db, file, "domain").place.expect_type();
        let parameter = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        )
        .map_bound_or_constraints(&db, |_| Some(TypeVarBoundOrConstraints::UpperBound(domain)));
        let unrelated = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("U"),
            TypeVarVariance::Invariant,
        );
        let visitor = ApplyTypeMappingVisitor::new_for_type_construction(&env);
        let expression = DeferredType::materialized_argument(
            &db,
            Type::any(),
            parameter,
            MaterializationKind::Top,
            &visitor,
        );
        let mapping = TypeMapping::ApplySpecialization(ApplySpecialization::Single(
            unrelated,
            Type::object(),
        ));
        assert_eq!(
            expression.apply_type_mapping_impl(&db, &mapping, TypeContext::default(), &visitor),
            expression
        );
        let Type::Deferred(expression) = expression else {
            panic!("a gradual bounded argument must remain deferred");
        };
        let Type::NominalInstance(domain_instance) = domain else {
            panic!("the declared domain must be a class instance");
        };
        let ClassType::Generic(domain_class) = domain_instance.class(&db, &env) else {
            panic!("the declared domain must retain its type argument");
        };
        // Materialization can produce `Recursive[object]`, although writing that
        // specialization in an annotation violates the declared bound and recovers to `Unknown`.
        let completed = Type::instance(
            &db,
            &env,
            ClassType::Generic(GenericAlias::new(
                &db,
                domain_class.origin(&db),
                domain_class
                    .specialization(&db)
                    .generic_context(&db)
                    .specialize(&db, [Type::object()].as_slice()),
            )),
        );
        assert!(
            expression
                .resolve(&db, &env)
                .is_equivalent_to(&db, &env, completed)
        );

        let int = KnownClass::Int.to_instance(&db, &env);
        let finite = parameter
            .map_bound_or_constraints(&db, |_| Some(TypeVarBoundOrConstraints::UpperBound(int)));
        let Type::Deferred(finite) = DeferredType::materialized_argument(
            &db,
            Type::any(),
            finite,
            MaterializationKind::Top,
            &visitor,
        ) else {
            panic!("a gradual bounded argument must remain deferred");
        };
        assert_eq!(finite.resolve(&db, &env), int);

        let callable = global_symbol(&db, file, "f").place.expect_type();
        let expression = DeferredType::materialized_argument(
            &db,
            callable,
            finite.parameter(&db),
            MaterializationKind::Top,
            &visitor,
        );
        assert_eq!(
            expression.apply_type_mapping_impl(&db, &mapping, TypeContext::default(), &visitor),
            expression
        );
    }
}
