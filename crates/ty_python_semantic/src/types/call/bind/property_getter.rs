//! Evaluate `property.__get__`, including calls to the property's stored getter.
//!
//! Class access and `TypeAliasType`/`TypeVar` reflection attributes such as `__name__` have direct
//! results. Other applicable accesses call the stored getter. Even if that call fails argument
//! checking, the outer `property.__get__` binding retains descriptor-call provenance and the
//! return type recovered from the getter's bindings. Failed getter bindings are retained in a
//! binding error for reporting.

use std::future::Future;

use super::effects::{BinderEffects, BinderLegacyEffect};
use super::{Binding, BindingError, Bindings, CallError, PropertyAccessorCallError};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::storage_quote::buffer_push_quote;
use crate::types::{DescriptorOrigin, KnownClass, KnownInstanceType, Type};
use crate::{Db, ProgramEnvironment};

/// Admits an inline continuation before constructing and polling it.
///
/// `SourceBinder` routes this admission through `SourceEffects::local_quoted`, which keeps `make`
/// in its continuation on refusal. The source invocation drains pending children before dropping
/// captured arguments or partial bindings that those children may borrow. One work unit and the
/// future's fixed representation bytes are charged before invoking `make`; semantic work and owned
/// payload storage inside the future require their own admissions.
pub(super) async fn run_with<'db, E, F, T>(
    effects: &E,
    make: impl FnOnce() -> F,
) -> Result<T, E::Error>
where
    E: BinderEffects<'db>,
    F: Future<Output = Result<T, E::Error>>,
{
    let future = effects.local(Some(1), Some(size_of::<F>()), make).await?;
    future.await
}

impl<'db> Binding<'db> {
    pub(super) async fn property_dunder_get_with<E: BinderEffects<'db>>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
        effects: &E,
    ) -> Result<(), E::Error> {
        match self.parameter_types() {
            [
                Some(property @ Type::PropertyInstance(_)),
                Some(instance),
                ..,
            ] if effects.type_is_none(db, *instance).await? => {
                let property = *property;
                self.set_property_return_with(property, effects).await?;
            }
            [
                Some(Type::PropertyInstance(property)),
                Some(Type::KnownInstance(KnownInstanceType::TypeAliasType(type_alias))),
                ..,
            ] if effects
                .operation(BinderLegacyEffect::KnownFunction, || {
                    property.getter(db).is_some_and(|getter| {
                        getter
                            .as_function_literal()
                            .is_some_and(|f| f.name(db) == "__name__")
                    })
                })
                .await? =>
            {
                let type_alias = *type_alias;
                effects
                    .operation(BinderLegacyEffect::KnownFunction, || {
                        self.set_return_type(Type::string_literal(db, type_alias.name(db)));
                    })
                    .await?;
            }
            [
                Some(Type::PropertyInstance(property)),
                Some(Type::KnownInstance(KnownInstanceType::TypeVar(typevar))),
                ..,
            ] => {
                let property = *property;
                let typevar = *typevar;
                effects
                    .operation(BinderLegacyEffect::KnownFunction, || {
                        match property.getter(db).and_then(Type::as_function_literal) {
                            Some(getter) if getter.name(db) == "__name__" => {
                                self.set_return_type(Type::string_literal(db, typevar.name(db)));
                            }
                            Some(getter) if getter.name(db) == "__bound__" => {
                                self.set_return_type(
                                    typevar
                                        .upper_bound(db, env)
                                        .unwrap_or_else(|| Type::none(db, env)),
                                );
                            }
                            Some(getter) if getter.name(db) == "__constraints__" => {
                                self.set_return_type(Type::heterogeneous_tuple(
                                    db,
                                    env,
                                    typevar.constraints(db, env).into_iter().flatten(),
                                ));
                            }
                            Some(getter) if getter.name(db) == "__default__" => {
                                self.set_return_type(typevar.default_type(db, env).unwrap_or_else(
                                    || KnownClass::NoDefaultType.to_instance(db, env),
                                ));
                            }
                            _ => {}
                        }
                    })
                    .await?;
            }
            [Some(Type::PropertyInstance(property)), Some(instance), ..] => {
                let property = *property;
                let instance = *instance;
                if let Some(getter) = effects.field(property.field_requests(db).getter()).await? {
                    run_with(effects, || {
                        self.check_property_getter_with(
                            db,
                            env,
                            getter,
                            instance,
                            1,
                            recursion_guard,
                            effects,
                        )
                    })
                    .await?;
                } else {
                    self.push_property_error_with(effects, || {
                        BindingError::PropertyHasNoGetter(property)
                    })
                    .await?;
                    self.set_property_return_with(Type::Never, effects).await?;
                }
            }
            [
                Some(property @ Type::NominalInstance(_)),
                Some(instance),
                ..,
            ] if effects.type_is_none(db, *instance).await? => {
                let property = *property;
                self.set_property_return_with(property, effects).await?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) async fn call_property_accessor_with<E: BinderEffects<'db>>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        accessor: Type<'db>,
        arguments: &[Type<'db>],
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
        effects: &E,
    ) -> Result<Result<Bindings<'db>, CallError<'db>>, E::Error> {
        let result = effects
            .property_accessor_call(db, env, accessor, arguments, recursion_guard)
            .await?;
        let bindings = match &result {
            Ok(bindings) => bindings,
            Err(CallError(_, bindings)) => bindings,
        };
        let origin = effects
            .bindings_origin(db, env, bindings, arguments)
            .await?;
        effects
            .local(Some(1), Some(size_of::<DescriptorOrigin<'db>>()), || {
                self.return_origin = origin;
            })
            .await?;
        Ok(result)
    }

    #[expect(clippy::too_many_arguments)]
    pub(super) async fn check_property_getter_with<E: BinderEffects<'db>>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        getter: Type<'db>,
        instance: Type<'db>,
        argument_index_offset: usize,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
        effects: &E,
    ) -> Result<(), E::Error> {
        let arguments = [instance];
        match run_with(effects, || {
            self.call_property_accessor_with(db, env, getter, &arguments, recursion_guard, effects)
        })
        .await?
        {
            Ok(bindings) => {
                let return_type = effects.bindings_return_type(db, env, &bindings).await?;
                self.set_property_return_with(return_type, effects).await?;
            }
            Err(CallError(_, bindings)) => {
                let return_type = effects.bindings_return_type(db, env, &bindings).await?;
                self.set_property_return_with(return_type, effects).await?;
                self.push_property_error_with(effects, || {
                    BindingError::PropertyGetterCallError(PropertyAccessorCallError {
                        bindings,
                        argument_index_offset,
                    })
                })
                .await?;
            }
        }
        Ok(())
    }

    async fn set_property_return_with<E: BinderEffects<'db>>(
        &mut self,
        return_type: Type<'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        effects
            .local(Some(1), Some(size_of::<Type<'db>>()), || {
                self.set_return_type(return_type);
            })
            .await
    }

    async fn push_property_error_with<E: BinderEffects<'db>>(
        &mut self,
        effects: &E,
        make: impl FnOnce() -> BindingError<'db>,
    ) -> Result<(), E::Error> {
        let quote = effects
            .local(Some(1), Some(0), || {
                buffer_push_quote::<BindingError<'db>>((
                    self.errors.len(),
                    self.errors.capacity(),
                    true,
                ))
            })
            .await?;
        effects
            .local(
                quote.map(|quote| quote.work),
                quote.and_then(|quote| quote.bytes.checked_add(size_of::<BindingError<'db>>())),
                || self.errors.push(make()),
            )
            .await
    }
}
