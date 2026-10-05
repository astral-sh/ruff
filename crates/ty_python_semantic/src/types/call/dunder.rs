//! Dunder lookup and invocation preserve Python call errors separately from interrupted work.

use std::convert::Infallible;
use std::future::{Future, ready};

use super::{Bindings, CallArguments, CallDunderError, CallError};
use crate::place::{DefinedPlace, Definedness, Place, PlaceAndQualifiers, Provenance};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::signatures::effects::legacy_inline;
use crate::types::{IntersectionType, MemberLookupPolicy, Type, TypeContext, UnionBuilder};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum DunderLookup {
    Implicit(MemberLookupPolicy),
    OnClass,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct DunderCallRequest<'name, 'db> {
    pub(in crate::types) receiver: Type<'db>,
    pub(in crate::types) name: &'name str,
    pub(in crate::types) lookup: DunderLookup,
    pub(in crate::types) tcx: TypeContext<'db>,
}

pub(in crate::types) type DunderCallResult<'db> = Result<Bindings<'db>, CallDunderError<'db>>;

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum DunderWork {
    IntersectionStorage { len: usize },
    MissingUnionElement,
    PossiblyUnboundStorage,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum DunderRead {
    IntersectionElements,
    UnionElements,
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

/// An invocation consumer supplies the builder used by `check_types`. Operational failure must
/// return through `Error`; only a completed call may return the inner Python call error.
pub(in crate::types) trait DunderEffects<'db>: sealed::Sealed {
    type Error;

    async fn admit(&self, work: DunderWork) -> Result<(), Self::Error>;

    async fn read<T>(
        &self,
        read: DunderRead,
        operation: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn finite_alternatives(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        intersection: IntersectionType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn lookup(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DunderCallRequest<'_, 'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;

    async fn bindings(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        callable: Type<'db>,
    ) -> Result<Bindings<'db>, Self::Error>;

    async fn match_parameters(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: Bindings<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Result<Bindings<'db>, Self::Error>;

    async fn check_types(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: Bindings<'db>,
        arguments: &CallArguments<'_, 'db>,
        tcx: TypeContext<'db>,
    ) -> Result<Result<Bindings<'db>, CallError<'db>>, Self::Error>;

    async fn call(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DunderCallRequest<'_, 'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Result<DunderCallResult<'db>, Self::Error>;

    async fn union_add(
        &self,
        builder: UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<UnionBuilder<'db>, Self::Error>;

    async fn union_build(&self, builder: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error>;

    async fn merge_intersection(
        &self,
        receiver: Type<'db>,
        bindings: Vec<Bindings<'db>>,
    ) -> Result<Bindings<'db>, Self::Error>;
}

impl<'name, 'db> DunderCallRequest<'name, 'db> {
    pub(in crate::types) fn implicit(
        receiver: Type<'db>,
        name: &'name str,
        tcx: TypeContext<'db>,
        policy: MemberLookupPolicy,
    ) -> Self {
        Self {
            receiver,
            name,
            lookup: DunderLookup::Implicit(policy),
            tcx,
        }
    }

    pub(in crate::types) fn on_class(
        receiver: Type<'db>,
        name: &'name str,
        tcx: TypeContext<'db>,
    ) -> Self {
        Self {
            receiver,
            name,
            lookup: DunderLookup::OnClass,
            tcx,
        }
    }

    pub(in crate::types) fn evaluate(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> DunderCallResult<'db> {
        legacy_inline(self.evaluate_with(db, env, arguments, &InlineDunderEffects))
    }

    pub(in crate::types) async fn evaluate_with<E: DunderEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
        effects: &E,
    ) -> Result<DunderCallResult<'db>, E::Error> {
        if matches!(self.lookup, DunderLookup::Implicit(_)) {
            if let Type::Intersection(intersection) = self.receiver {
                if let Some(receiver) = effects.finite_alternatives(db, env, intersection).await? {
                    return effects
                        .call(db, env, Self { receiver, ..self }, arguments)
                        .await;
                }

                // Call each positive element separately and intersect the resulting bindings.
                // Intersecting bound methods first can collapse the callable itself to Never.
                // TODO: reconsider this after https://github.com/astral-sh/ty/issues/2428.
                // `object` need not be added for an empty positive list: it defines none of the
                // dunders called here without MRO_NO_OBJECT_FALLBACK, such as __iter__ or __bool__.
                let positive = effects
                    .read(DunderRead::IntersectionElements, || {
                        intersection.positive(db)
                    })
                    .await?;
                effects
                    .admit(DunderWork::IntersectionStorage {
                        len: positive.len(),
                    })
                    .await?;
                let mut successful_bindings = Vec::with_capacity(positive.len());
                let mut last_error = None;
                let mut provenance = Provenance::Unknown;
                for &receiver in positive {
                    match effects
                        .call(db, env, Self { receiver, ..self }, arguments)
                        .await?
                    {
                        Ok(bindings) => successful_bindings.push(bindings),
                        Err(error) => {
                            provenance = provenance.or(error.provenance());
                            last_error = Some(error);
                        }
                    }
                }
                if successful_bindings.is_empty() {
                    // TODO: report all failed elements rather than only the last one.
                    return Ok(Err(last_error
                        .unwrap_or(CallDunderError::MethodNotAvailable)
                        .with_provenance(provenance)));
                }
                return Ok(Ok(effects
                    .merge_intersection(self.receiver, successful_bindings)
                    .await?));
            }

            if let Type::Union(union) = self.receiver {
                // Preserve missing union members for the possibly-unbound diagnostic instead of
                // losing their identities in an aggregate member lookup.
                let elements = effects
                    .read(DunderRead::UnionElements, || union.elements(db))
                    .await?;
                let mut builder = UnionBuilder::new(db, env);
                let mut unbound_on = Vec::new();
                let mut any_defined = false;
                let mut possibly_undefined = false;
                let mut provenance = Provenance::Unknown;
                for &receiver in elements {
                    match effects
                        .lookup(db, env, Self { receiver, ..self })
                        .await?
                        .place
                    {
                        Place::Defined(DefinedPlace {
                            ty,
                            definedness,
                            provenance: member_provenance,
                            ..
                        }) => {
                            builder = effects.union_add(builder, ty).await?;
                            any_defined = true;
                            possibly_undefined |= definedness == Definedness::PossiblyUndefined;
                            provenance = provenance.or(member_provenance);
                        }
                        Place::Undefined => {
                            effects.admit(DunderWork::MissingUnionElement).await?;
                            unbound_on.push(receiver);
                            possibly_undefined = true;
                        }
                    }
                }
                if !any_defined {
                    return Ok(Err(CallDunderError::MethodNotAvailable));
                }
                let callable = effects.union_build(builder).await?;
                return self
                    .invoke_with(
                        db,
                        env,
                        arguments,
                        effects,
                        callable,
                        provenance,
                        possibly_undefined,
                        unbound_on,
                    )
                    .await;
            }
        }

        match effects.lookup(db, env, self).await?.place {
            Place::Defined(DefinedPlace {
                ty,
                definedness,
                provenance,
                ..
            }) => {
                self.invoke_with(
                    db,
                    env,
                    arguments,
                    effects,
                    ty,
                    provenance,
                    definedness == Definedness::PossiblyUndefined,
                    Vec::new(),
                )
                .await
            }
            Place::Undefined => Ok(Err(CallDunderError::MethodNotAvailable)),
        }
    }

    #[expect(clippy::too_many_arguments)]
    async fn invoke_with<E: DunderEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
        effects: &E,
        callable: Type<'db>,
        provenance: Provenance<'db>,
        possibly_undefined: bool,
        unbound_on: Vec<Type<'db>>,
    ) -> Result<DunderCallResult<'db>, E::Error> {
        let bindings = effects.bindings(db, env, callable).await?;
        let bindings = effects
            .match_parameters(db, env, bindings, arguments)
            .await?;
        let bindings = match effects
            .check_types(db, env, bindings, arguments, self.tcx)
            .await?
        {
            Ok(bindings) => bindings,
            Err(CallError(kind, bindings)) => {
                return Ok(Err(CallDunderError::CallError(kind, bindings, provenance)));
            }
        };
        if possibly_undefined {
            effects.admit(DunderWork::PossiblyUnboundStorage).await?;
            return Ok(Err(CallDunderError::PossiblyUnbound {
                bindings: Box::new(bindings),
                unbound_on: (!unbound_on.is_empty()).then(|| unbound_on.into_boxed_slice()),
            }));
        }
        Ok(Ok(bindings))
    }
}

pub(in crate::types) struct InlineDunderEffects;

impl sealed::Sealed for InlineDunderEffects {}

impl<'db> DunderEffects<'db> for InlineDunderEffects {
    type Error = Infallible;

    async fn admit(&self, _work: DunderWork) -> Result<(), Infallible> {
        Ok(())
    }

    async fn read<T>(
        &self,
        _read: DunderRead,
        operation: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(operation())
    }

    fn finite_alternatives(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        intersection: IntersectionType<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Infallible>> {
        ready(Ok(intersection.finite_alternative_union(db, env)))
    }

    fn lookup(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DunderCallRequest<'_, 'db>,
    ) -> impl Future<Output = Result<PlaceAndQualifiers<'db>, Infallible>> {
        ready(Ok(match request.lookup {
            // Implicit dunder calls never access instance members.
            DunderLookup::Implicit(policy) => request.receiver.member_lookup_with_policy(
                db,
                env,
                request.name,
                policy | MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            ),
            DunderLookup::OnClass => request.receiver.member(db, env, request.name),
        }))
    }

    fn bindings(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        callable: Type<'db>,
    ) -> impl Future<Output = Result<Bindings<'db>, Infallible>> {
        ready(Ok(callable.bindings(db, env)))
    }

    fn match_parameters(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: Bindings<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> impl Future<Output = Result<Bindings<'db>, Infallible>> {
        ready(Ok(bindings.match_parameters(db, env, arguments)))
    }

    fn check_types(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: Bindings<'db>,
        arguments: &CallArguments<'_, 'db>,
        tcx: TypeContext<'db>,
    ) -> impl Future<Output = Result<Result<Bindings<'db>, CallError<'db>>, Infallible>> {
        let constraints = ConstraintSetBuilder::new();
        ready(Ok(bindings.check_types(
            db,
            env,
            &constraints,
            arguments,
            tcx,
            &[],
        )))
    }

    fn call(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DunderCallRequest<'_, 'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> impl Future<Output = Result<DunderCallResult<'db>, Infallible>> {
        ready(Ok(request.evaluate(db, env, arguments)))
    }

    fn union_add(
        &self,
        builder: UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<UnionBuilder<'db>, Infallible>> {
        ready(Ok(builder.add(ty)))
    }

    fn union_build(
        &self,
        builder: UnionBuilder<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Infallible>> {
        ready(Ok(builder.build()))
    }

    fn merge_intersection(
        &self,
        receiver: Type<'db>,
        bindings: Vec<Bindings<'db>>,
    ) -> impl Future<Output = Result<Bindings<'db>, Infallible>> {
        ready(Ok(Bindings::from_intersection(receiver, bindings)))
    }
}
