//! Exact tuple class conversion through canonical class, specialization, and union operations.

use salsa::execution_probe::{RunError, RunResult};

use super::class_selection::{FixedFieldBorrow, FixedFieldCopy};
use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::types::class::{
    ApplyClassSpecializationEffects, apply_class_specialization_with,
    interpret_class_literal_lookup,
};
use crate::types::generics::defaults::default_specialization_with_effects;
use crate::types::tuple::TupleType;
use crate::types::tuple::class_conversion::{
    TupleClassEffects, tuple_class_specialization_with, tuple_class_with,
};
use crate::types::{
    ClassType, GenericContext, KnownClass, Specialization, StaticClassLiteral, Type,
};

/// Inputs retained while the class's own generic context is resolved.
struct TupleClassApplication<'env, 'db> {
    env: &'env ProgramEnvironment<'db>,
    tuple: TupleType<'db>,
    cycle: Option<salsa::Id>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Converts an exact tuple to its builtin class for `to_class_type` or its cycle initializer.
    pub(in crate::types::infer) async fn infer_tuple_class(
        &self,
        tuple: TupleType<'db>,
        cycle: Option<salsa::Id>,
    ) -> RunResult<ClassType<'db>> {
        self.allocate_future(|| tuple_class_with(tuple, cycle, self))
            .await?
            .await
    }

    /// Admits work and callback/result storage, then executes a fixed tuple-conversion action.
    async fn tuple_class_local<T>(
        &self,
        work: usize,
        requested_bytes: usize,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.local_with_fixed_transfers(work, requested_bytes, action)
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TupleClassEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn environment(&self, tuple: TupleType<'db>) -> RunResult<ProgramEnvironment<'db>> {
        let program = self
            .field_with_profile(
                tuple
                    .field_requests(self.access.endpoint().field_request_context())
                    .program(),
                &FixedFieldCopy,
            )
            .await?;
        self.tuple_class_local(2, 0, || {
            self.check_program(program)?;
            Ok(ProgramEnvironment::from_program(program))
        })
        .await?
    }

    async fn tuple_class(
        &self,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<StaticClassLiteral<'db>> {
        let result = self
            .access
            .known_class_lookup(self.program, KnownClass::Tuple)
            .await?;
        self.tuple_class_local(3, 0, || {
            interpret_class_literal_lookup(result).ok_or(RunError::Contract(
                "Typeshed should always have a `tuple` class in `builtins.pyi`",
            ))
        })
        .await?
    }

    async fn apply_class(
        &self,
        class: StaticClassLiteral<'db>,
        env: &ProgramEnvironment<'db>,
        tuple: TupleType<'db>,
        cycle: Option<salsa::Id>,
    ) -> RunResult<ClassType<'db>> {
        let input = self
            .tuple_class_local(3, size_of::<ClassType<'db>>(), || TupleClassApplication {
                env,
                tuple,
                cycle,
            })
            .await?;
        self.allocate_future(|| apply_class_specialization_with(class, input, self))
            .await?
            .await
    }

    async fn is_single_parameter(&self, context: GenericContext<'db>) -> RunResult<bool> {
        let variables = self
            .field_with_profile(
                context.variables_request(self.access.endpoint().field_request_context()),
                &FixedFieldBorrow,
            )
            .await?;
        self.tuple_class_local(2, 0, || variables.len() == 1).await
    }

    async fn element_union(
        &self,
        env: &ProgramEnvironment<'db>,
        tuple: TupleType<'db>,
    ) -> RunResult<Type<'db>> {
        let spec = self
            .field_with_profile(
                tuple
                    .field_requests(self.access.endpoint().field_request_context())
                    .tuple(),
                &FixedFieldBorrow,
            )
            .await?;
        let elements = self
            .tuple_class_local(6, 0, || spec.class_elements())
            .await?;
        self.tuple_elements_union(env, elements.prefix, elements.variable, elements.suffix)
            .await
    }

    async fn divergent_element(&self, id: salsa::Id) -> RunResult<Type<'db>> {
        self.tuple_class_local(1, 0, || Type::divergent(id)).await
    }

    async fn specialize_tuple(
        &self,
        context: GenericContext<'db>,
        element: Type<'db>,
        tuple: TupleType<'db>,
    ) -> RunResult<Specialization<'db>> {
        // Prepay disposal of the one-element box before a later field read or interning
        // operation can stop while it is retained.
        let types = self
            .tuple_class_local(4, size_of::<[Type<'db>; 1]>(), || {
                Box::<[Type<'db>]>::from([element])
            })
            .await?;
        self.access
            .intern_specialization(context, types, None, Some(tuple))
            .await
    }

    async fn default_specialization(
        &self,
        context: GenericContext<'db>,
    ) -> RunResult<Specialization<'db>> {
        self.allocate_future(|| {
            default_specialization_with_effects(self.db(), context, Some(KnownClass::Tuple), self)
        })
        .await?
        .await
    }
}

impl<'env, 'run, 'db: 'run, A: SourceAccess<'run, 'db>>
    ApplyClassSpecializationEffects<'db, TupleClassApplication<'env, 'db>>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.access.class_generic_context(class).await
    }

    async fn specialize(
        &self,
        context: GenericContext<'db>,
        input: TupleClassApplication<'env, 'db>,
    ) -> RunResult<Specialization<'db>> {
        self.allocate_future(|| {
            tuple_class_specialization_with(context, input.env, input.tuple, input.cycle, self)
        })
        .await?
        .await
    }

    async fn generic_alias(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> RunResult<ClassType<'db>> {
        let alias = self
            .access
            .intern_generic_alias(class, specialization)
            .await?;
        self.tuple_class_local(1, 0, || ClassType::Generic(alias))
            .await
    }
}
