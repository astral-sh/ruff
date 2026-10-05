//! Dependencies of checking an entire call, including constructor and native call behavior.

use std::convert::Infallible;
use std::future::{Future, ready};

use super::checking::BindingsCheckStep;
use super::constructor::ConstructorBinding;
use super::effects::{BinderEffects, InlineBinderEffects};
use super::{Bindings, CallArguments, CallError, CallErrorKind, CallableItem, CheckTypesMode};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::{Type, TypeContext};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy)]
pub(super) struct CheckContext<'a, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'a ProgramEnvironment<'db>,
    pub(super) constraints: &'a ConstraintSetBuilder<'db>,
    pub(super) arguments: &'a CallArguments<'a, 'db>,
    pub(super) tcx: TypeContext<'db>,
    pub(super) dataclass_field_specifiers: &'a [Type<'db>],
}

/// Constructor and native behavior may start further calls. Their completion is required before
/// filtering successful bindings or deciding whether the enclosing call failed.
pub(super) trait BindingsEffects<'db>: BinderEffects<'db> {
    async fn constructor(
        &self,
        binding: &mut ConstructorBinding<'db>,
        context: CheckContext<'_, 'db>,
        mode: CheckTypesMode,
    ) -> Result<(), Self::Error>;

    async fn known_cases(
        &self,
        bindings: &mut Bindings<'db>,
        context: CheckContext<'_, 'db>,
    ) -> Result<(), Self::Error>;

    async fn downstream(
        &self,
        binding: &mut ConstructorBinding<'db>,
        context: CheckContext<'_, 'db>,
    ) -> Result<(), Self::Error>;

    async fn step_work(&self, _bindings: &Bindings<'db>) -> Result<Option<usize>, Self::Error> {
        Ok(Some(0))
    }

    async fn admitted_step<T>(
        &self,
        work: Option<usize>,
        operation: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        let _ = work;
        self.step(operation)
    }

    fn step<T>(&self, operation: impl FnOnce() -> T) -> Result<T, Self::Error>;
}

impl<'db> BindingsEffects<'db> for InlineBinderEffects<'_, 'db> {
    fn constructor(
        &self,
        binding: &mut ConstructorBinding<'db>,
        context: CheckContext<'_, 'db>,
        mode: CheckTypesMode,
    ) -> impl Future<Output = Result<(), Infallible>> {
        binding.check_types(context, mode, self);
        ready(Ok(()))
    }

    fn known_cases(
        &self,
        bindings: &mut Bindings<'db>,
        context: CheckContext<'_, 'db>,
    ) -> impl Future<Output = Result<(), Infallible>> {
        bindings.evaluate_known_cases(
            context.db,
            context.env,
            context.arguments,
            context.dataclass_field_specifiers,
            self.recursion_guard,
        );
        ready(Ok(()))
    }

    fn downstream(
        &self,
        binding: &mut ConstructorBinding<'db>,
        context: CheckContext<'_, 'db>,
    ) -> impl Future<Output = Result<(), Infallible>> {
        binding.check_downstream_constructor(
            context.db,
            context.env,
            context.constraints,
            context.arguments,
            context.tcx,
            context.dataclass_field_specifiers,
            self.recursion_guard,
        );
        ready(Ok(()))
    }

    fn step<T>(&self, operation: impl FnOnce() -> T) -> Result<T, Infallible> {
        Ok(operation())
    }
}

impl<'db> Bindings<'db> {
    pub(super) async fn check_types_with_effects<E: BindingsEffects<'db>>(
        mut self,
        context: CheckContext<'_, 'db>,
        effects: &E,
    ) -> Result<Result<Self, CallError<'db>>, E::Error> {
        let result = self
            .check_types_impl_with_effects(context, CheckTypesMode::Finalize, effects)
            .await?;
        Ok(match result {
            Ok(()) => Ok(self),
            Err(error) => Err(CallError(error, Box::new(self))),
        })
    }

    pub(super) async fn check_types_impl_with_effects<E: BindingsEffects<'db>>(
        &mut self,
        context: CheckContext<'_, 'db>,
        mode: CheckTypesMode,
        effects: &E,
    ) -> Result<Result<(), CallErrorKind>, E::Error> {
        let work = effects.step_work(self).await?;
        let mut step = effects
            .admitted_step(work, || BindingsCheckStep::start(self, mode))
            .await?;
        loop {
            step = match step {
                BindingsCheckStep::Item(pending) => {
                    match pending.item(self) {
                        CallableItem::Regular(binding) => {
                            binding
                                .check_types_with(
                                    context.db,
                                    context.env,
                                    context.constraints,
                                    context.arguments,
                                    context.tcx,
                                    effects,
                                )
                                .await?;
                        }
                        CallableItem::Constructor(binding) => {
                            effects
                                .constructor(binding, context, pending.mode())
                                .await?;
                        }
                    }
                    let work = effects.step_work(self).await?;
                    effects.admitted_step(work, || pending.resume(self)).await?
                }
                BindingsCheckStep::KnownCases(pending) => {
                    effects.known_cases(self, context).await?;
                    let work = effects.step_work(self).await?;
                    effects
                        .admitted_step(work, || pending.resume(context.db, self))
                        .await?
                }
                BindingsCheckStep::Downstream(pending) => {
                    if let Some(constructor) = pending.item(self).as_constructor_mut() {
                        effects.downstream(constructor, context).await?;
                    }
                    let work = effects.step_work(self).await?;
                    effects
                        .admitted_step(work, || pending.resume(context.db, self))
                        .await?
                }
                BindingsCheckStep::Complete(result) => return Ok(result),
            };
        }
    }
}
