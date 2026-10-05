use std::convert::Infallible;

use ruff_db::parsed::parsed_module;
use ruff_python_ast::PythonVersion;

use super::StaticClassLiteral;
use crate::types::call::{CallError, CallErrorKind};
use crate::types::class::metaclass_selection::MetaclassSelectionResult;
use crate::types::class::{ClassMetaclass, MetaclassError, MetaclassErrorKind};
use crate::types::class_base::conversion::ClassBaseConversion;
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_sync};
use crate::types::mro::root::InlineMroRootEffects;
use crate::types::{
    CallArguments, ClassBase, ClassLiteral, ClassType, DataclassTransformerParams, GenericAlias,
    KnownClass, MetaclassCandidate, MetaclassTransformInfo, Specialization, StaticMroError,
    SubclassOfType, Type,
};
use crate::{Db, ProgramEnvironment};

/// Retains a lazy base cursor and its program contexts for one metaclass selection.
/// After cycle checks, `pending` preserves the first converted base while explicit-metaclass
/// inference runs; later bases are not converted until selection consumes them.
pub(in crate::types) struct MetaclassBases<'db> {
    pub class: StaticClassLiteral<'db>,
    /// File-level environment used to select and reconcile metaclasses.
    pub env: ProgramEnvironment<'db>,
    /// Class-body environment used to convert explicit base types into class bases.
    pub base_env: ProgramEnvironment<'db>,
    pub bases: &'db [Type<'db>],
    pub next: usize,
    pub pending: Option<ClassBase<'db>>,
    pub has_protocol_fallback: bool,
    #[cfg(all(test, feature = "experimental-analysis"))]
    pub retirement_observer: Option<fn()>,
}

#[cfg(all(test, feature = "experimental-analysis"))]
impl Drop for MetaclassBases<'_> {
    fn drop(&mut self) {
        if let Some(observer) = self.retirement_observer {
            observer();
        }
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousInnerMetaclassEffects)]
    pub(in crate::types) trait InnerMetaclassEffects<'db> {
        type Error;

        /// Funds the reducer's fixed candidate, comparison, and result transfers.
        #[operation(local)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        /// Borrows explicit bases and retains their conversion and selection contexts.
        #[operation(child)]
        async fn bases(&self, class: StaticClassLiteral<'db>) -> Result<MetaclassBases<'db>, Self::Error>;
        #[operation(local)]
        async fn take_pending(&self, bases: &mut MetaclassBases<'db>) -> Result<Option<ClassBase<'db>>, Self::Error>;
        #[operation(local)]
        async fn save_pending(&self, bases: &mut MetaclassBases<'db>, base: Option<ClassBase<'db>>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_raw(&self, bases: &mut MetaclassBases<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn convert_base(&self, bases: &MetaclassBases<'db>, ty: Type<'db>) -> Result<Option<ClassBase<'db>>, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_base(&self, bases: &mut MetaclassBases<'db>) -> Result<Option<ClassBase<'db>>, Self::Error>;
        #[operation(child)]
        async fn base_metaclass(&self, bases: &MetaclassBases<'db>, base: ClassBase<'db>) -> Result<ClassMetaclass<'db>, Self::Error>;
        #[operation(local)]
        async fn record_fallback(&self, bases: &mut MetaclassBases<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_selected(&self, bases: &mut MetaclassBases<'db>) -> Result<Option<(ClassBase<'db>, Type<'db>)>, Self::Error>;
        #[operation(child)]
        async fn inheritance_cycle(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn mro_is_cycle(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn explicit_metaclass(&self, class: StaticClassLiteral<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn specialization_has_typevars(&self, env: &ProgramEnvironment<'db>, alias: GenericAlias<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn known_type(&self, env: &ProgramEnvironment<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn class_type(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;

        /// Tests for the unconstrained class-object type returned by metaclass recovery.
        #[operation(local)]
        async fn is_unknown_metaclass(&self, ty: Type<'db>) -> Result<bool, Self::Error>;

        /// Compares class identities and specialization handles without traversing their contents.
        #[operation(local)]
        async fn same_class(&self, left: ClassType<'db>, right: ClassType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn call_metaclass(&self, bases: &MetaclassBases<'db>, metaclass: Type<'db>) -> Result<MetaclassSelectionResult<'db>, Self::Error>;
        #[operation(child)]
        async fn most_derived(&self, env: &ProgramEnvironment<'db>, candidate: ClassType<'db>, other: ClassType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn transform_params(&self, metaclass: ClassType<'db>) -> Result<Option<DataclassTransformerParams<'db>>, Self::Error>;
        #[operation(child)]
        async fn finish(&self, bases: &MetaclassBases<'db>, metaclass: ClassType<'db>) -> Result<ClassMetaclass<'db>, Self::Error>;
    }

    /// Returns the next class or Protocol base, preserving a previously peeked conversion.
    #[synchronous(next_metaclass_base_sync)]
    #[capabilities(effects = InnerMetaclassEffects)]
    #[passive_values()]
    pub(in crate::types) async fn next_metaclass_base_with<'db, E: InnerMetaclassEffects<'db>>(
        bases: &mut MetaclassBases<'db>, effects: &E,
    ) -> Result<Option<ClassBase<'db>>, E::Error> {
        if let Some(base) = effects.take_pending(bases).await? {
            return Ok(Some(base));
        }
        #[cursor_loop]
        while let Some(ty) = effects.next_raw(bases).await? {
            if let Some(base) = effects.convert_base(bases, ty).await?
                && matches!(base, ClassBase::Class(_) | ClassBase::Protocol)
            {
                return Ok(Some(base));
            }
        }
        Ok(None)
    }

    /// Returns the next base metaclass that constrains selection, recording skipped fallbacks.
    #[synchronous(next_selected_metaclass_sync)]
    #[capabilities(effects = InnerMetaclassEffects)]
    #[passive_values()]
    pub(in crate::types) async fn next_selected_metaclass_with<'db, E: InnerMetaclassEffects<'db>>(
        bases: &mut MetaclassBases<'db>, effects: &E,
    ) -> Result<Option<(ClassBase<'db>, Type<'db>)>, E::Error> {
        #[cursor_loop]
        while let Some(base) = effects.next_base(bases).await? {
            match effects.base_metaclass(bases, base).await? {
                ClassMetaclass::Selected(metaclass) => return Ok(Some((base, metaclass))),
                ClassMetaclass::ProtocolFallback => effects.record_fallback(bases).await?,
            }
        }
        Ok(None)
    }

    /// Selects a metaclass in base order, retaining cycle, callable, conflict, and transform results.
    ///
    /// Here `Child` inherits `Meta` through `Parent`; that supplying base is retained even when
    /// the metaclass's transform parameters come from an ancestor of `Meta`:
    ///
    /// ```python
    /// from typing import dataclass_transform
    ///
    /// @dataclass_transform()
    /// class MetaBase(type): ...
    /// class Meta(MetaBase): ...
    /// class Parent(metaclass=Meta): ...
    /// class Child(Parent): ...
    /// ```
    #[synchronous(inner_metaclass_sync)]
    #[capabilities(effects = InnerMetaclassEffects)]
    #[passive_values(ClassMetaclass::Selected, SubclassOfType::subclass_of_unknown, Err, MetaclassError, MetaclassErrorKind, MetaclassCandidate, MetaclassTransformInfo)]
    pub(in crate::types) async fn inner_metaclass_with<'db, E: InnerMetaclassEffects<'db>>(
        class: StaticClassLiteral<'db>, effects: &E,
    ) -> Result<MetaclassSelectionResult<'db>, E::Error> {
        let mut bases = effects.bases(class).await?;
        // Identify the class's own metaclass (or take the first base class's metaclass).
        let first = effects.next_base(&mut bases).await?;
        effects.checkpoint().await?;
        let has_first = match first {
            Some(_) => true,
            None => false,
        };
        if (has_first && effects.inheritance_cycle(class).await?)
            || effects.mro_is_cycle(class).await?
        {
            // We emit diagnostics for cyclic class definitions elsewhere.
            // Avoid attempting to infer the metaclass if the class is cyclically defined.
            return Ok(Ok((ClassMetaclass::Selected(SubclassOfType::subclass_of_unknown()), None)));
        }
        effects.save_pending(&mut bases, first).await?;
        let explicit_metaclass = effects.explicit_metaclass(class).await?;

        // Generic metaclasses parameterized by type variables are not supported.
        // `metaclass=Meta[int]` is fine, but `metaclass=Meta[T]` is not.
        // See: https://typing.python.org/en/latest/spec/generics.html#generic-metaclasses
        if let Some(Type::GenericAlias(alias)) = explicit_metaclass
            && effects.specialization_has_typevars(&bases.env, alias).await?
        {
            return Ok(Err(MetaclassError { kind: MetaclassErrorKind::GenericMetaclass }));
        }
        let (metaclass, base) = if let Some(metaclass) = explicit_metaclass {
            (metaclass, None)
        } else if let Some((base, metaclass)) = effects.next_selected(&mut bases).await? {
            (metaclass, Some(base))
        } else {
            (effects.known_type(&bases.env).await?, None)
        };
        effects.checkpoint().await?;
        #[passive_state]
        let mut candidate = if let Some(metaclass) = effects.class_type(metaclass).await? {
            MetaclassCandidate { metaclass, base }
        } else {
            return effects.call_metaclass(&bases, metaclass).await;
        };

        // Reconcile all base classes' metaclasses with the candidate metaclass.
        //
        // See:
        // - https://docs.python.org/3/reference/datamodel.html#determining-the-appropriate-metaclass
        // - https://github.com/python/cpython/blob/83ba8c2bba834c0b92de669cac16fcda17485e0e/Objects/typeobject.c#L3629-L3663
        #[cursor_loop]
        while let Some(entry) = effects.next_selected(&mut bases).await? {
            let (base_class, metaclass) = entry;
            effects.checkpoint().await?;
            if effects.is_unknown_metaclass(metaclass).await? {
                return Ok(Ok((ClassMetaclass::Selected(metaclass), None)));
            }
            let Some(metaclass) = effects.class_type(metaclass).await? else {
                continue;
            };
            if let Some(selected) = effects.most_derived(&bases.env, candidate.metaclass, metaclass).await? {
                let Some(metaclass) = effects.class_type(selected).await? else {
                    return Ok(Ok((ClassMetaclass::Selected(selected), None)));
                };
                if effects.same_class(metaclass, candidate.metaclass).await? {
                    continue;
                }
                candidate = MetaclassCandidate { metaclass, base: Some(base_class) };
                continue;
            }
            let explicit_metaclass = match explicit_metaclass {
                Some(metaclass) => effects.class_type(metaclass).await?,
                None => None,
            };
            return Ok(Err(MetaclassError { kind: MetaclassErrorKind::Conflict {
                candidate, base_metaclass: metaclass, base: base_class, explicit_metaclass,
            } }));
        }
        let params = effects.transform_params(candidate.metaclass).await?;
        effects.checkpoint().await?;
        let transform_info = match params {
            Some(params) => Some(MetaclassTransformInfo { params, from_explicit_metaclass: match candidate.base { None => true, Some(_) => false } }),
            None => None,
        };
        let metaclass = effects.finish(&bases, candidate.metaclass).await?;
        Ok(Ok((metaclass, transform_info)))
    }

    #[synchronous(SynchronousInheritedTransformEffects)]
    pub(in crate::types) trait InheritedTransformEffects<'db> {
        type Error;
        #[operation(source)]
        async fn own_params(&self, class: StaticClassLiteral<'db>) -> Result<Option<DataclassTransformerParams<'db>>, Self::Error>;
        #[operation(local)]
        async fn mro(&self, class: StaticClassLiteral<'db>, specialization: Option<Specialization<'db>>) -> Result<MroCursor<'db>, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_mro(&self, cursor: &mut MroCursor<'db>) -> Result<Option<ClassBase<'db>>, Self::Error>;
        #[operation(source)]
        async fn static_literal(&self, base: ClassBase<'db>) -> Result<Option<StaticClassLiteral<'db>>, Self::Error>;
    }

    /// Finds the class's own transform parameters, or those of the first static MRO ancestor with parameters.
    /// The cursor uses the incoming specialization and skips its first entry, which is the class itself.
    #[synchronous(inherited_transform_sync)]
    #[capabilities(effects = InheritedTransformEffects)]
    #[passive_values()]
    pub(in crate::types) async fn inherited_transform_with<'db, E: InheritedTransformEffects<'db>>(
        class: StaticClassLiteral<'db>, specialization: Option<Specialization<'db>>, effects: &E,
    ) -> Result<Option<DataclassTransformerParams<'db>>, E::Error> {
        if let Some(params) = effects.own_params(class).await? {
            return Ok(Some(params));
        }
        let mut mro = effects.mro(class, specialization).await?;
        effects.next_mro(&mut mro).await?;
        #[cursor_loop]
        while let Some(base) = effects.next_mro(&mut mro).await? {
            if let Some(class) = effects.static_literal(base).await?
                && let Some(params) = effects.own_params(class).await?
            {
                return Ok(Some(params));
            }
        }
        Ok(None)
    }
}

/// Supplies ordinary semantic children to the shared metaclass and transform reducers.
pub(super) struct OrdinaryInnerMetaclass<'db>(pub &'db dyn Db);

/// Reports whether the known class's metaclass is guaranteed to be `type` at this Python version.
pub(in crate::types) fn known_type_metaclass(known: KnownClass, version: PythonVersion) -> bool {
    known.has_known_type_metaclass(version)
}

#[cfg(all(test, feature = "experimental-analysis"))]
pub(in crate::types) const fn error_for_native_test(
    kind: MetaclassErrorKind<'_>,
) -> MetaclassError<'_> {
    MetaclassError { kind }
}

impl<'db> SynchronousInnerMetaclassEffects<'db> for OrdinaryInnerMetaclass<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn bases(&self, class: StaticClassLiteral<'db>) -> Result<MetaclassBases<'db>, Infallible> {
        let program_file = class.program_file(self.0);
        let _python_file = program_file.python_file(self.0);
        let env = ProgramEnvironment::from_file(program_file);
        let base_env = ProgramEnvironment::from_scope(class.body_scope(self.0));
        Ok(MetaclassBases {
            class,
            env,
            base_env,
            bases: class.explicit_bases(self.0),
            next: 0,
            pending: None,
            has_protocol_fallback: false,
            #[cfg(all(test, feature = "experimental-analysis"))]
            retirement_observer: None,
        })
    }

    fn take_pending(
        &self,
        bases: &mut MetaclassBases<'db>,
    ) -> Result<Option<ClassBase<'db>>, Infallible> {
        Ok(bases.pending.take())
    }

    fn save_pending(
        &self,
        bases: &mut MetaclassBases<'db>,
        base: Option<ClassBase<'db>>,
    ) -> Result<(), Infallible> {
        bases.pending = base;
        Ok(())
    }

    fn next_raw(&self, bases: &mut MetaclassBases<'db>) -> Result<Option<Type<'db>>, Infallible> {
        let next = bases.bases.get(bases.next).copied();
        bases.next += usize::from(next.is_some());
        Ok(next)
    }

    fn convert_base(
        &self,
        bases: &MetaclassBases<'db>,
        ty: Type<'db>,
    ) -> Result<Option<ClassBase<'db>>, Infallible> {
        Ok(ClassBaseConversion::from_type(ty).resolve(
            self.0,
            &bases.base_env,
            Some(ClassLiteral::Static(bases.class)),
        ))
    }

    fn next_base(
        &self,
        bases: &mut MetaclassBases<'db>,
    ) -> Result<Option<ClassBase<'db>>, Infallible> {
        next_metaclass_base_sync(bases, self)
    }

    fn base_metaclass(
        &self,
        bases: &MetaclassBases<'db>,
        base: ClassBase<'db>,
    ) -> Result<ClassMetaclass<'db>, Infallible> {
        Ok(base.inferred_metaclass(self.0, &bases.env, ClassLiteral::Static(bases.class)))
    }

    fn record_fallback(&self, bases: &mut MetaclassBases<'db>) -> Result<(), Infallible> {
        bases.has_protocol_fallback = true;
        Ok(())
    }

    fn next_selected(
        &self,
        bases: &mut MetaclassBases<'db>,
    ) -> Result<Option<(ClassBase<'db>, Type<'db>)>, Infallible> {
        next_selected_metaclass_sync(bases, self)
    }

    fn inheritance_cycle(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.inheritance_cycle(self.0).is_some())
    }

    fn mro_is_cycle(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class
            .try_mro(self.0, None)
            .is_err_and(StaticMroError::is_cycle))
    }

    fn explicit_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        let module =
            parsed_module(self.0, class.program_file(self.0).python_file(self.0)).load(self.0);
        Ok(class.explicit_metaclass(self.0, &module))
    }

    fn specialization_has_typevars(
        &self,
        env: &ProgramEnvironment<'db>,
        alias: GenericAlias<'db>,
    ) -> Result<bool, Infallible> {
        Ok(alias
            .specialization(self.0)
            .types(self.0)
            .iter()
            .any(|ty| ty.has_typevar_or_typevar_instance(self.0, env)))
    }

    fn known_type(&self, env: &ProgramEnvironment<'db>) -> Result<Type<'db>, Infallible> {
        Ok(KnownClass::Type.to_class_literal(self.0, env))
    }

    fn class_type(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Infallible> {
        Ok(ty.to_class_type(self.0))
    }

    fn is_unknown_metaclass(&self, ty: Type<'db>) -> Result<bool, Infallible> {
        Ok(ty == SubclassOfType::subclass_of_unknown())
    }

    fn same_class(&self, left: ClassType<'db>, right: ClassType<'db>) -> Result<bool, Infallible> {
        Ok(left == right)
    }

    fn call_metaclass(
        &self,
        bases: &MetaclassBases<'db>,
        metaclass: Type<'db>,
    ) -> Result<MetaclassSelectionResult<'db>, Infallible> {
        let db = self.0;
        let env = &bases.env;
        let name = Type::string_literal(db, bases.class.name(db));
        let explicit_bases = Type::heterogeneous_tuple(db, env, bases.class.explicit_bases(db));
        let namespace = KnownClass::Dict.to_specialized_instance(
            db,
            env,
            &[KnownClass::Str.to_instance(db, env), Type::any()],
        );
        // TODO: Other keyword arguments?
        let arguments = CallArguments::positional([name, explicit_bases, namespace]);
        let result = match metaclass.try_call(db, env, &arguments) {
            Ok(bindings) => Ok(bindings.return_type(db, env)),
            Err(CallError(CallErrorKind::NotCallable, bindings)) => Err(MetaclassError {
                kind: MetaclassErrorKind::NotCallable(bindings.callable_type()),
            }),
            // TODO we should also check for binding errors that would indicate the metaclass
            // does not accept the right arguments
            Err(CallError(CallErrorKind::BindingError, bindings)) => {
                Ok(bindings.return_type(db, env))
            }
            Err(CallError(CallErrorKind::PossiblyNotCallable, _)) => Err(MetaclassError {
                kind: MetaclassErrorKind::PartlyNotCallable(metaclass),
            }),
        };
        Ok(result.map(|ty| (ClassMetaclass::Selected(ty.to_meta_type(db, env)), None)))
    }

    fn most_derived(
        &self,
        env: &ProgramEnvironment<'db>,
        candidate: ClassType<'db>,
        other: ClassType<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(candidate.most_derived_metaclass(self.0, env, other))
    }

    fn transform_params(
        &self,
        metaclass: ClassType<'db>,
    ) -> Result<Option<DataclassTransformerParams<'db>>, Infallible> {
        Ok(metaclass
            .static_class_literal(self.0)
            .and_then(|(class, specialization)| {
                class.inherited_dataclass_transformer_params(self.0, specialization)
            }))
    }

    fn finish(
        &self,
        bases: &MetaclassBases<'db>,
        metaclass: ClassType<'db>,
    ) -> Result<ClassMetaclass<'db>, Infallible> {
        let fallback = bases.has_protocol_fallback
            && !bases.class.known(self.0).is_some_and(|known| {
                known.has_known_type_metaclass(bases.env.python_version(self.0))
            });
        Ok(ClassMetaclass::with_protocol_fallback(
            self.0,
            metaclass.into(),
            fallback,
        ))
    }
}

impl<'db> SynchronousInheritedTransformEffects<'db> for OrdinaryInnerMetaclass<'db> {
    type Error = Infallible;

    fn own_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<DataclassTransformerParams<'db>>, Infallible> {
        Ok(class.dataclass_transformer_params(self.0))
    }

    fn mro(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<MroCursor<'db>, Infallible> {
        Ok(MroCursor::new(class.into(), specialization))
    }

    fn next_mro(&self, cursor: &mut MroCursor<'db>) -> Result<Option<ClassBase<'db>>, Infallible> {
        mro_next_sync(
            self.0,
            cursor,
            MroDirection::Forward,
            &InlineMroRootEffects::new(self.0),
        )
    }

    fn static_literal(
        &self,
        base: ClassBase<'db>,
    ) -> Result<Option<StaticClassLiteral<'db>>, Infallible> {
        Ok(base
            .into_class()
            .and_then(|class| class.static_class_literal(self.0).map(|(class, _)| class)))
    }
}
