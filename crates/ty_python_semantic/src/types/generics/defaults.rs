//! Ordered generic argument filling and construction of default specializations.

use std::borrow::Cow;
use std::convert::Infallible;

#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::execution_probe::{InternedValues, RegistryBuilder, RunResult};
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::interned::FiniteInternedConfiguration;
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::{ApplySpecialization, GenericContext, Specialization};
use crate::types::tuple::TupleType;
use crate::types::{
    BoundTypeVarIdentity, BoundTypeVarInstance, KnownClass, Parameters, Type, TypeContext,
    TypeMapping, TypeVarKind,
};
use crate::{Db, Program, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum DefaultSpecializationWork {
    Context,
    Arity,
    Allocate { len: usize },
    Advance,
    Default,
    MapDefault { prefix: usize },
    Kind,
    UnknownTuple,
    UnknownParamSpec,
    Append { len: usize, capacity: usize },
    Box { len: usize, capacity: usize },
    Intern { len: usize, borrowed: bool },
    Resume,
    Publish,
}

pub(in crate::types) trait DefaultSpecializationEffects<'db> {
    type Error;

    fn checkpoint(&self, work: DefaultSpecializationWork) -> Result<(), Self::Error>;

    fn default_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    fn map_default(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        default: Type<'db>,
        context: GenericContext<'db>,
        prefix: &[Type<'db>],
    ) -> Result<Type<'db>, Self::Error>;

    fn unknown_tuple(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<TupleType<'db>, Self::Error>;

    fn unknown_paramspec(&self, db: &'db dyn Db) -> Result<Type<'db>, Self::Error>;

    fn intern_specialization(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        types: Cow<'_, [Type<'db>]>,
        tuple: Option<TupleType<'db>>,
    ) -> Result<Specialization<'db>, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DefaultSpecializationOperation {
    DefaultType,
    DefaultMapping,
    UnknownTuple,
    UnknownParamSpec,
}

pub(in crate::types) type DefaultVariableCursor<'db> = std::iter::Copied<
    ordermap::map::Values<'db, BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>,
>;

pub(in crate::types) struct DefaultSpecializationFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousDefaultSpecializationConstructionEffects)]
    pub(in crate::types) trait DefaultSpecializationConstructionEffects<'db> {
        type Error;
        #[operation(local)]
        async fn checkpoint(&self, work: DefaultSpecializationWork) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn context_len(&self, context: GenericContext<'db>) -> Result<usize, Self::Error>;
        #[operation(source)]
        async fn context_program(&self, context: GenericContext<'db>) -> Result<Program<'db>, Self::Error>;
        #[operation(child)]
        async fn specialize_missing(&self, db: &'db dyn Db, context: GenericContext<'db>, len: usize) -> Result<Specialization<'db>, Self::Error>;
        #[operation(source)]
        async fn specialization_types(&self, specialization: Specialization<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(child)]
        async fn unknown_tuple(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Result<TupleType<'db>, Self::Error>;
        #[operation(local)]
        async fn intern_specialization(&self, db: &'db dyn Db, context: GenericContext<'db>, types: Cow<'_, [Type<'db>]>, tuple: Option<TupleType<'db>>) -> Result<Specialization<'db>, Self::Error>;
        #[operation(local)]
        async fn owned_types(&self, types: Box<[Type<'db>]>) -> Result<Cow<'db, [Type<'db>]>, Self::Error>;
    }

    #[synchronous(SynchronousDefaultArgumentEffects)]
    pub(in crate::types) trait DefaultArgumentEffects<'db, I>: DefaultSpecializationConstructionEffects<'db> {
        type Cursor;
        type Buffer;
        #[operation(local)]
        async fn arguments(&self, input: I) -> Result<Self::Cursor, Self::Error>;
        #[operation(source)]
        async fn context_variables(&self, context: GenericContext<'db>) -> Result<DefaultVariableCursor<'db>, Self::Error>;
        #[operation(local)]
        async fn input_len(&self, input: &Self::Cursor) -> Result<usize, Self::Error>;
        #[operation(source)]
        async fn check_arity(&self, context: GenericContext<'db>, input: &Self::Cursor) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn new_buffer(&self, len: usize) -> Result<Self::Buffer, Self::Error>;
        #[operation(local)]
        async fn buffer_len(&self, buffer: &Self::Buffer) -> Result<usize, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_argument(&self, input: &mut Self::Cursor, variables: &mut DefaultVariableCursor<'db>) -> Result<Option<(Option<Type<'db>>, BoundTypeVarInstance<'db>)>, Self::Error>;
        #[operation(source)]
        async fn variable_kind(&self, variable: BoundTypeVarInstance<'db>) -> Result<TypeVarKind, Self::Error>;
        #[operation(local)]
        async fn append(&self, types: &mut Self::Buffer, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish_buffer(&self, types: Self::Buffer) -> Result<Box<[Type<'db>]>, Self::Error>;
        #[operation(child)]
        async fn fill_supplied(&self, db: &'db dyn Db, context: GenericContext<'db>, input: I) -> Result<Box<[Type<'db>]>, Self::Error>;
        #[operation(child)]
        async fn default_type(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, variable: BoundTypeVarInstance<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn map_default(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, default: Type<'db>, context: GenericContext<'db>, prefix: &Self::Buffer) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn unknown_paramspec(&self, db: &'db dyn Db) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl DefaultSpecializationFacts {
        fn environment<'db>(&self, program: Program<'db>) -> ProgramEnvironment<'db> { ProgramEnvironment::from_program(program) }
        fn len(&self, types: &[Type<'_>]) -> usize { types.len() }
        fn boxed_slice<'a, 'db>(&self, types: &'a Box<[Type<'db>]>) -> &'a [Type<'db>] { types.as_ref() }
        fn tuple_type<'db>(&self, tuple: TupleType<'db>) -> Type<'db> { Type::tuple(tuple) }
        fn unknown<'db>(&self) -> Type<'db> { Type::unknown() }
        fn is_tuple(&self, known: Option<KnownClass>) -> bool { known == Some(KnownClass::Tuple) }
    }

    #[synchronous(fill_in_defaults_effects_sync)]
    #[capabilities(effects = DefaultArgumentEffects, facts = DefaultSpecializationFacts)]
    #[passive_values(DefaultSpecializationWork::Context, DefaultSpecializationWork::Arity, DefaultSpecializationWork::Default, DefaultSpecializationWork::Resume, DefaultSpecializationWork::MapDefault, DefaultSpecializationWork::Kind, DefaultSpecializationWork::UnknownTuple, DefaultSpecializationWork::UnknownParamSpec, DefaultSpecializationWork::Publish)]
    pub(in crate::types) async fn fill_in_defaults_with_effects<'db, I, E: DefaultArgumentEffects<'db, I>>(
        db: &'db dyn Db, context: GenericContext<'db>, input: I, facts: DefaultSpecializationFacts, effects: &E,
    ) -> Result<Box<[Type<'db>]>, E::Error> {
        effects.checkpoint(DefaultSpecializationWork::Context).await?;
        let program = effects.context_program(context).await?;
        let env = facts.environment(program);
        let mut input = effects.arguments(input).await?;
        let mut variables = effects.context_variables(context).await?;
        effects.checkpoint(DefaultSpecializationWork::Arity).await?;
        effects.check_arity(context, &input).await?;

        // Typevars can have other typevars as their default values, e.g.
        //
        // ```py
        // class C[T, U = T]: ...
        // ```
        //
        // If there is a mapping for `T`, we want to map `U` to that type, not to `T`.
        // Fill each argument in order so defaults can use the preceding arguments.
        let len = effects.input_len(&input).await?;
        let mut expanded = effects.new_buffer(len).await?;
        #[cursor_loop]
        while let Some(argument) = effects.next_argument(&mut input, &mut variables).await? {
            let (ty, typevar) = argument;
            let ty = if let Some(ty) = ty {
                ty
            } else {
                effects.checkpoint(DefaultSpecializationWork::Default).await?;
                let default = effects.default_type(db, &env, typevar).await?;
                effects.checkpoint(DefaultSpecializationWork::Resume).await?;
                if let Some(default) = default {
                    // Typevars are only allowed to refer to earlier typevars in their defaults.
                    // This is statically enforced for PEP 695 contexts, and explicitly required
                    // for legacy contexts.
                    let prefix = effects.buffer_len(&expanded).await?;
                    effects.checkpoint(DefaultSpecializationWork::MapDefault { prefix }).await?;
                    let mapped = effects.map_default(db, &env, default, context, &expanded).await?;
                    effects.checkpoint(DefaultSpecializationWork::Resume).await?;
                    mapped
                } else {
                    effects.checkpoint(DefaultSpecializationWork::Kind).await?;
                    match effects.variable_kind(typevar).await? {
                        TypeVarKind::LegacyTypeVarTuple | TypeVarKind::Pep695TypeVarTuple => {
                            effects.checkpoint(DefaultSpecializationWork::UnknownTuple).await?;
                            let tuple = effects.unknown_tuple(db, &env).await?;
                            effects.checkpoint(DefaultSpecializationWork::Resume).await?;
                            facts.tuple_type(tuple)
                        }
                        TypeVarKind::LegacyParamSpec | TypeVarKind::Pep695ParamSpec => {
                            effects.checkpoint(DefaultSpecializationWork::UnknownParamSpec).await?;
                            let callable = effects.unknown_paramspec(db).await?;
                            effects.checkpoint(DefaultSpecializationWork::Resume).await?;
                            callable
                        }
                        _ => facts.unknown(),
                    }
                }
            };
            effects.append(&mut expanded, ty).await?;
        }
        let expanded = effects.finish_buffer(expanded).await?;
        effects.checkpoint(DefaultSpecializationWork::Publish).await?;
        Ok(expanded)
    }

    #[synchronous(specialize_partial_effects_sync)]
    #[capabilities(effects = DefaultArgumentEffects, facts = DefaultSpecializationFacts)]
    #[passive_values(DefaultSpecializationWork::Intern, DefaultSpecializationWork::Resume, DefaultSpecializationWork::Publish)]
    pub(in crate::types) async fn specialize_partial_with_effects<'db, I, E: DefaultArgumentEffects<'db, I>>(
        db: &'db dyn Db, context: GenericContext<'db>, input: I, facts: DefaultSpecializationFacts, effects: &E,
    ) -> Result<Specialization<'db>, E::Error> {
        let types = effects.fill_supplied(db, context, input).await?;
        effects.checkpoint(DefaultSpecializationWork::Intern { len: facts.len(facts.boxed_slice(&types)), borrowed: false }).await?;
        let types = effects.owned_types(types).await?;
        let specialization = effects.intern_specialization(db, context, types, None).await?;
        effects.checkpoint(DefaultSpecializationWork::Resume).await?;
        effects.checkpoint(DefaultSpecializationWork::Publish).await?;
        Ok(specialization)
    }

    #[synchronous(default_specialization_effects_sync)]
    #[capabilities(effects = DefaultSpecializationConstructionEffects, facts = DefaultSpecializationFacts)]
    #[passive_values(DefaultSpecializationWork::Context, DefaultSpecializationWork::UnknownTuple, DefaultSpecializationWork::Intern, DefaultSpecializationWork::Resume, DefaultSpecializationWork::Publish, Cow::Borrowed)]
    async fn default_specialization_body_with_effects<'db, E: DefaultSpecializationConstructionEffects<'db>>(
        db: &'db dyn Db, context: GenericContext<'db>, known_class: Option<KnownClass>, facts: DefaultSpecializationFacts, effects: &E,
    ) -> Result<Specialization<'db>, E::Error> {
        effects.checkpoint(DefaultSpecializationWork::Context).await?;
        let len = effects.context_len(context).await?;
        let partial = effects.specialize_missing(db, context, len).await?;
        let specialization = if facts.is_tuple(known_class) {
            effects.checkpoint(DefaultSpecializationWork::Context).await?;
            let program = effects.context_program(context).await?;
            let env = facts.environment(program);
            let types = effects.specialization_types(partial).await?;
            effects.checkpoint(DefaultSpecializationWork::UnknownTuple).await?;
            let tuple = effects.unknown_tuple(db, &env).await?;
            effects.checkpoint(DefaultSpecializationWork::Resume).await?;
            effects.checkpoint(DefaultSpecializationWork::Intern { len: facts.len(types), borrowed: true }).await?;
            let specialization = effects.intern_specialization(db, context, Cow::Borrowed(types), Some(tuple)).await?;
            effects.checkpoint(DefaultSpecializationWork::Resume).await?;
            specialization
        } else { partial };
        effects.checkpoint(DefaultSpecializationWork::Publish).await?;
        Ok(specialization)
    }
}

pub(in crate::types) async fn default_specialization_with_effects<
    'db,
    E: DefaultSpecializationConstructionEffects<'db>,
>(
    db: &'db dyn Db,
    context: GenericContext<'db>,
    known_class: Option<KnownClass>,
    effects: &E,
) -> Result<Specialization<'db>, E::Error> {
    default_specialization_body_with_effects(
        db,
        context,
        known_class,
        DefaultSpecializationFacts,
        effects,
    )
    .await
}

pub(in crate::types) fn fill_in_defaults_with<'db, I, E>(
    db: &'db dyn Db,
    context: GenericContext<'db>,
    types: I,
    effects: &E,
) -> Result<Box<[Type<'db>]>, E::Error>
where
    I: IntoIterator<Item = Option<Type<'db>>>,
    I::IntoIter: ExactSizeIterator,
    E: DefaultSpecializationEffects<'db>,
{
    fill_in_defaults_effects_sync(
        db,
        context,
        types,
        DefaultSpecializationFacts,
        &InlineDefaults { db, effects },
    )
}

pub(in crate::types) fn specialize_partial_with<'db, I, E>(
    db: &'db dyn Db,
    context: GenericContext<'db>,
    types: I,
    effects: &E,
) -> Result<Specialization<'db>, E::Error>
where
    I: IntoIterator<Item = Option<Type<'db>>>,
    I::IntoIter: ExactSizeIterator,
    E: DefaultSpecializationEffects<'db>,
{
    specialize_partial_effects_sync(
        db,
        context,
        types,
        DefaultSpecializationFacts,
        &InlineDefaults { db, effects },
    )
}

pub(in crate::types) fn default_specialization_with<'db, E: DefaultSpecializationEffects<'db>>(
    db: &'db dyn Db,
    context: GenericContext<'db>,
    known_class: Option<KnownClass>,
    effects: &E,
) -> Result<Specialization<'db>, E::Error> {
    default_specialization_effects_sync(
        db,
        context,
        known_class,
        DefaultSpecializationFacts,
        &InlineDefaults { db, effects },
    )
}

struct InlineDefaults<'db, 'effects, E> {
    db: &'db dyn Db,
    effects: &'effects E,
}

impl<'db, E: DefaultSpecializationEffects<'db>>
    SynchronousDefaultSpecializationConstructionEffects<'db> for InlineDefaults<'db, '_, E>
{
    type Error = E::Error;
    fn checkpoint(&self, work: DefaultSpecializationWork) -> Result<(), Self::Error> {
        self.effects.checkpoint(work)
    }
    fn context_len(&self, context: GenericContext<'db>) -> Result<usize, Self::Error> {
        Ok(context.len(self.db))
    }
    fn context_program(&self, context: GenericContext<'db>) -> Result<Program<'db>, Self::Error> {
        Ok(context.program(self.db))
    }
    fn specialize_missing(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        len: usize,
    ) -> Result<Specialization<'db>, Self::Error> {
        specialize_partial_effects_sync(
            db,
            context,
            std::iter::repeat_n(None, len),
            DefaultSpecializationFacts,
            self,
        )
    }
    fn specialization_types(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(specialization.types(self.db))
    }
    fn unknown_tuple(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<TupleType<'db>, Self::Error> {
        self.effects.unknown_tuple(db, env)
    }
    fn intern_specialization(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        types: Cow<'_, [Type<'db>]>,
        tuple: Option<TupleType<'db>>,
    ) -> Result<Specialization<'db>, Self::Error> {
        self.effects
            .intern_specialization(db, context, types, tuple)
    }
    fn owned_types(&self, types: Box<[Type<'db>]>) -> Result<Cow<'db, [Type<'db>]>, Self::Error> {
        Ok(Cow::Owned(types.into_vec()))
    }
}

impl<'db, I, E> SynchronousDefaultArgumentEffects<'db, I> for InlineDefaults<'db, '_, E>
where
    I: IntoIterator<Item = Option<Type<'db>>>,
    I::IntoIter: ExactSizeIterator,
    E: DefaultSpecializationEffects<'db>,
{
    type Cursor = I::IntoIter;
    type Buffer = Vec<Type<'db>>;
    fn arguments(&self, input: I) -> Result<Self::Cursor, Self::Error> {
        Ok(input.into_iter())
    }
    fn context_variables(
        &self,
        context: GenericContext<'db>,
    ) -> Result<DefaultVariableCursor<'db>, Self::Error> {
        Ok(context.variables(self.db))
    }
    fn input_len(&self, input: &Self::Cursor) -> Result<usize, Self::Error> {
        Ok(input.len())
    }
    fn check_arity(
        &self,
        context: GenericContext<'db>,
        input: &Self::Cursor,
    ) -> Result<(), Self::Error> {
        assert_eq!(context.len(self.db), input.len());
        Ok(())
    }
    fn new_buffer(&self, len: usize) -> Result<Self::Buffer, Self::Error> {
        self.effects
            .checkpoint(DefaultSpecializationWork::Allocate { len })?;
        Ok(Vec::with_capacity(len))
    }
    fn buffer_len(&self, buffer: &Self::Buffer) -> Result<usize, Self::Error> {
        Ok(buffer.len())
    }
    fn next_argument(
        &self,
        input: &mut Self::Cursor,
        variables: &mut DefaultVariableCursor<'db>,
    ) -> Result<Option<(Option<Type<'db>>, BoundTypeVarInstance<'db>)>, Self::Error> {
        self.effects
            .checkpoint(DefaultSpecializationWork::Advance)?;
        Ok(input
            .next()
            .and_then(|ty| variables.next().map(|variable| (ty, variable))))
    }
    fn variable_kind(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarKind, Self::Error> {
        Ok(variable.kind(self.db))
    }
    fn append(&self, types: &mut Self::Buffer, ty: Type<'db>) -> Result<(), Self::Error> {
        self.effects.checkpoint(DefaultSpecializationWork::Append {
            len: types.len(),
            capacity: types.capacity(),
        })?;
        types.push(ty);
        Ok(())
    }
    fn finish_buffer(&self, types: Self::Buffer) -> Result<Box<[Type<'db>]>, Self::Error> {
        self.effects.checkpoint(DefaultSpecializationWork::Box {
            len: types.len(),
            capacity: types.capacity(),
        })?;
        Ok(types.into_boxed_slice())
    }
    fn fill_supplied(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        input: I,
    ) -> Result<Box<[Type<'db>]>, Self::Error> {
        fill_in_defaults_effects_sync(db, context, input, DefaultSpecializationFacts, self)
    }
    fn default_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.effects.default_type(db, env, variable)
    }
    fn map_default(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        default: Type<'db>,
        context: GenericContext<'db>,
        prefix: &Self::Buffer,
    ) -> Result<Type<'db>, Self::Error> {
        self.effects.map_default(db, env, default, context, prefix)
    }
    fn unknown_paramspec(&self, db: &'db dyn Db) -> Result<Type<'db>, Self::Error> {
        self.effects.unknown_paramspec(db)
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl FiniteInternedConfiguration for Specialization<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        specialization_field_work(&fields.1, |_| Ok(())).ok()
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        specialization_field_work(&fields.1, |units| fuel.consume(units))
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
fn specialization_field_work(
    types: &[Type<'_>],
    mut consume: impl FnMut(usize) -> Result<(), QuoteError>,
) -> Result<usize, QuoteError> {
    consume(1)?;
    consume(types.len())?;
    types.iter().try_fold(4usize, |work, ty| {
        work.checked_add(1)
            .and_then(|work| work.checked_add(ty.inline_payload_bytes()))
            .ok_or(QuoteError::Overflow)
    })
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) type SpecializationValues<'db> =
    InternedValues<'db, Specialization<'static>, ()>;

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn register_specialization_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<SpecializationValues<'db>> {
    registry.finite_interned_values_with_memos(Specialization::ingredient(db.zalsa()), ())
}

pub(in crate::types) struct Unrestricted;

impl<'db> DefaultSpecializationEffects<'db> for Unrestricted {
    type Error = Infallible;

    fn checkpoint(&self, _work: DefaultSpecializationWork) -> Result<(), Infallible> {
        Ok(())
    }

    fn default_type(
        &self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(variable.default_type(db))
    }

    fn map_default(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        default: Type<'db>,
        context: GenericContext<'db>,
        prefix: &[Type<'db>],
    ) -> Result<Type<'db>, Infallible> {
        let specialization = ApplySpecialization::Partial {
            generic_context: context,
            types: prefix.into(),
            skip: None,
        };
        Ok(default.apply_type_mapping(
            db,
            env,
            &TypeMapping::ApplySpecialization(specialization),
            TypeContext::default(),
        ))
    }

    fn unknown_tuple(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<TupleType<'db>, Infallible> {
        Ok(TupleType::homogeneous(db, env, Type::unknown()))
    }

    fn unknown_paramspec(&self, db: &'db dyn Db) -> Result<Type<'db>, Infallible> {
        Ok(Type::paramspec_value_callable(db, Parameters::unknown()))
    }

    fn intern_specialization(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        types: Cow<'_, [Type<'db>]>,
        tuple: Option<TupleType<'db>>,
    ) -> Result<Specialization<'db>, Infallible> {
        Ok(Specialization::new(db, context, types, None, tuple))
    }
}
