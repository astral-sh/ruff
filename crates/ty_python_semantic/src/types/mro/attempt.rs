//! Installed MRO effects that keep source recovery out of semantic decisions.

use ruff_python_ast as ast;
use salsa::plumbing::AsId;
use std::borrow::Cow;
use std::collections::VecDeque;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;

use super::base::{
    BaseMroFacts, BaseMroStart, BaseMroWork, SynchronousBaseMroEffects, collect_base_mro_sync,
    collect_single_base_mro_sync,
};
use super::c3::{C3Work, SynchronousC3Effects, c3_merge_sync};
use super::collection::base::{collect_start_sync, collect_start_with_root_sync};
use super::collection::{MroCollectionWork, SynchronousMroCollectionEffects};
use super::construction::{StaticMroFacts, StaticMroWork, SynchronousStaticMroEffects};
use super::field_reads::MroFieldReads;
use super::iteration::{MroIterationWork, SynchronousMroIterationEffects, full_mro_with};
use super::root::{
    MroRootFacts, MroRootWork, MroTailRequest, SynchronousMroRootEffects,
    apply_optional_class_specialization_sync,
};
use super::{Mro, StaticMroError, StaticMroErrorKind};
use crate::types::class::base_entries::{ClassBaseEntryEffects, ClassBaseEntryWork};
use crate::types::class::context::inherited::{InheritedContextEffects, InheritedContextWork};
use crate::types::class::context::{
    ClassContextBaseCursor, ClassContextEffects, ClassContextWork, legacy_generic_context_with,
};
use crate::types::class_base::{ClassBase, ClassBaseConversion, ClassBaseDependency};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::generics::context_construction::{
    ContextConstructionControl, ContextConstructionWork,
};
use crate::types::generics::defaults::{DefaultSpecializationEffects, DefaultSpecializationWork};
use crate::types::generics::tuple_runtime::{
    TupleRuntimeControl, TupleRuntimeWork, tuple_runtime_element_specialization_with,
};
use crate::types::generics::{GenericContext, Specialization};
use crate::types::legacy_typevars::{
    LegacyTypeVarDependency, LegacyTypeVarEffects, LegacyTypeVarWork, find_legacy_typevars_with,
};
use crate::types::source_read::{SourceReadControl, read_source};
use crate::types::tuple::{TupleSpec, TupleType};
use crate::types::{
    BoundTypeVarInstance, ClassType, FindLegacyTypeVarsVisitor, GenericAlias, KnownClass,
    Parameters, StaticClassLiteral, Type,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod cycles;

#[cfg(test)]
mod inherited_context;

#[cfg(test)]
mod default_specialization;

#[cfg(test)]
mod source_context;

#[cfg(test)]
mod base_conversion;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum UnsupportedMroOperation {
    TypeVarDefault,
    DefaultMapping,
    DynamicMro,
    BaseTuple,
    InheritedContext,
    BaseConversion,
    ErrorDetails,
    MissingObject,
}

pub(in crate::types) struct AttemptMroEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> AttemptMroEffects<'db> {
    pub(in crate::types) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }

    #[cfg_attr(test, track_caller)]
    fn admit(&self, units: usize) -> Result<(), Incomplete> {
        expansion_probe::charge_work(self.db, units)
    }

    fn unsupported<T>(&self, operation: UnsupportedMroOperation) -> Result<T, Incomplete> {
        self.check()?;
        Err(expansion_probe::refuse(
            self.db,
            Incomplete::UnsupportedMroOperation(operation),
        ))
    }

    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Incomplete> {
        read_source(self, || class.explicit_bases(self.db))
    }

    fn object_base(&self, env: &ProgramEnvironment<'db>) -> Result<ClassBase<'db>, Incomplete> {
        self.admit(1)?;
        let Some(object) = KnownClass::Object.try_to_class_literal_with(self.db, env, self)? else {
            return self.unsupported(UnsupportedMroOperation::MissingObject);
        };
        Ok(ClassBase::Class(ClassType::NonGeneric(object.into())))
    }
}

impl SourceReadControl for AttemptMroEffects<'_> {
    type Error = Incomplete;

    fn check(&self) -> Result<(), Incomplete> {
        expansion_probe::continue_work(self.db)
    }
}

impl crate::types::class::base_entries::sealed::Sealed for AttemptMroEffects<'_> {}

impl<'db> ClassBaseEntryEffects<'db> for AttemptMroEffects<'db> {
    type Error = Incomplete;

    fn checkpoint(&self, work: ClassBaseEntryWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        self.admit(match work {
            ClassBaseEntryWork::Capacity {
                prefix_len,
                capacity,
            } => prefix_len.saturating_add(capacity),
            ClassBaseEntryWork::Publish { len } | ClassBaseEntryWork::BoxOutput { len } => {
                len.saturating_add(1)
            }
            _ => 1,
        })
    }

    fn expression_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Incomplete> {
        read_source(self, || {
            crate::types::definition_expression_type(self.db, definition, expression)
        })
    }

    fn tuple_spec(
        &self,
        _: Definition<'db>,
        _: Type<'db>,
    ) -> Result<Option<Cow<'db, TupleSpec<'db>>>, Incomplete> {
        self.unsupported(UnsupportedMroOperation::BaseTuple)
    }
}

impl super::root::sealed::Sealed for AttemptMroEffects<'_> {}

impl<'db> MroRootFacts<'db> for AttemptMroEffects<'db> {
    type Error = Incomplete;
}

impl<'db> SynchronousMroRootEffects<'db> for AttemptMroEffects<'db> {
    fn generic_alias(
        &self,
        class: crate::types::StaticClassLiteral<'db>,
        specialization: crate::types::generics::Specialization<'db>,
    ) -> Result<crate::types::ClassType<'db>, Self::Error> {
        Ok(crate::types::ClassType::Generic(
            crate::types::GenericAlias::new(self.db, class, specialization),
        ))
    }

    fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Incomplete> {
        read_source(self, || class.generic_context(self.db))
    }

    fn checkpoint(&self, work: MroRootWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        self.admit(1)
    }

    fn default_class_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Incomplete> {
        crate::types::class::default_class_specialization_with(self.db, class, self)
    }

    fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Incomplete> {
        expansion_probe::observe(expansion_probe::Observation::TupleNormalization(
            specialization.as_id(),
        ));
        tuple_runtime_element_specialization_with(self.db, specialization, self)
    }
}

impl TupleRuntimeControl for AttemptMroEffects<'_> {
    type Error = Incomplete;

    fn checkpoint(&self, work: TupleRuntimeWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        self.admit(match work {
            TupleRuntimeWork::Intern => 3,
            TupleRuntimeWork::Inspect | TupleRuntimeWork::Publish => 1,
        })
    }
}

impl<'db> DefaultSpecializationEffects<'db> for AttemptMroEffects<'db> {
    type Error = Incomplete;

    fn checkpoint(&self, work: DefaultSpecializationWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        self.admit(match work {
            DefaultSpecializationWork::Allocate { len } => len.saturating_add(1),
            DefaultSpecializationWork::Append { len, capacity } if len == capacity => {
                len.saturating_add(capacity).saturating_add(1)
            }
            DefaultSpecializationWork::Box { len, capacity } => {
                len.saturating_add(capacity).saturating_add(1)
            }
            DefaultSpecializationWork::Intern { len, borrowed } => len
                .saturating_mul(if borrowed { 2 } else { 1 })
                .saturating_add(1),
            DefaultSpecializationWork::MapDefault { prefix } => prefix.saturating_add(1),
            DefaultSpecializationWork::UnknownTuple => 2,
            DefaultSpecializationWork::UnknownParamSpec => 4,
            _ => 1,
        })
    }

    fn default_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Incomplete> {
        let (default, skipped_lazy) = variable.typevar(db).default_for_visitor(db, env, false);
        if default.is_some() || skipped_lazy {
            return self.unsupported(UnsupportedMroOperation::TypeVarDefault);
        }
        read_source(self, || variable.default_type(db))
    }

    fn map_default(
        &self,
        _: &'db dyn Db,
        _: &ProgramEnvironment<'db>,
        _: Type<'db>,
        _: GenericContext<'db>,
        _: &[Type<'db>],
    ) -> Result<Type<'db>, Incomplete> {
        self.unsupported(UnsupportedMroOperation::DefaultMapping)
    }

    fn unknown_tuple(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<TupleType<'db>, Incomplete> {
        Ok(TupleType::homogeneous(db, env, Type::unknown()))
    }

    fn unknown_paramspec(&self, db: &'db dyn Db) -> Result<Type<'db>, Incomplete> {
        Ok(Type::paramspec_value_callable(db, Parameters::unknown()))
    }

    fn intern_specialization(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        types: Cow<'_, [Type<'db>]>,
        tuple: Option<TupleType<'db>>,
    ) -> Result<Specialization<'db>, Incomplete> {
        Ok(Specialization::new(db, context, types, None, tuple))
    }
}

impl<'db> SynchronousMroIterationEffects<'db> for AttemptMroEffects<'db> {
    fn iteration_checkpoint(&self, work: MroIterationWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        self.admit(1)
    }

    fn full_mro(&self, request: MroTailRequest<'db>) -> Result<&'db Mro<'db>, Incomplete> {
        if !matches!(request, MroTailRequest::Static(..)) {
            return self.unsupported(UnsupportedMroOperation::DynamicMro);
        }
        full_mro_with(self.db, request, self)
    }
}

impl<'db> SynchronousMroCollectionEffects<'db> for AttemptMroEffects<'db> {
    fn collection_checkpoint(&self, work: MroCollectionWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        let units = match work {
            MroCollectionWork::Append { len, capacity } if len == capacity => len.saturating_add(1),
            MroCollectionWork::BoxOutput { len, .. } => len,
            _ => 1,
        };
        self.admit(units)
    }
}

impl crate::types::class::context::sealed::Sealed for AttemptMroEffects<'_> {}

impl super::base::sealed::Sealed for AttemptMroEffects<'_> {}

impl<'db> BaseMroFacts<'db> for AttemptMroEffects<'db> {
    type Error = Incomplete;
}

impl<'db> SynchronousBaseMroEffects<'db> for AttemptMroEffects<'db> {
    fn alias_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Incomplete> {
        Ok(super::field_reads::MroFieldReads::new(self.db).alias_origin(alias))
    }

    fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Incomplete> {
        Ok(super::field_reads::MroFieldReads::new(self.db).alias_specialization(alias))
    }

    fn object_base(&self, env: &ProgramEnvironment<'db>) -> Result<ClassBase<'db>, Incomplete> {
        self.object_base(env)
    }

    fn checkpoint(&self, work: BaseMroWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        self.admit(1)
    }

    fn compose_specialization(
        &self,
        base: Specialization<'db>,
        additional: Specialization<'db>,
    ) -> Result<Specialization<'db>, Incomplete> {
        crate::types::mapping::attempt::compose_specialization(self.db, base, additional)
    }

    fn collect_start(
        &self,
        start: BaseMroStart<'db>,
    ) -> Result<VecDeque<ClassBase<'db>>, Incomplete> {
        collect_start_sync(self.db, start, self)
    }

    fn collect_start_with_root(
        &self,
        root: ClassType<'db>,
        start: BaseMroStart<'db>,
    ) -> Result<Mro<'db>, Incomplete> {
        collect_start_with_root_sync(self.db, root, start, self)
    }
}

impl super::c3::sealed::Sealed for AttemptMroEffects<'_> {}

impl SynchronousC3Effects for AttemptMroEffects<'_> {
    type Error = Incomplete;

    fn mro_identity<'db>(
        &self,
        fields: MroFieldReads<'db>,
        base: ClassBase<'db>,
    ) -> Result<Type<'db>, Incomplete> {
        Ok(fields.mro_identity(base))
    }

    fn checkpoint(&self, work: C3Work) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        self.admit(match work {
            C3Work::OutputCapacity { entries } => entries,
            C3Work::RetainSequences { len } | C3Work::BoxOutput { len, .. } => len,
            C3Work::IdentityComparison { todo_bytes } | C3Work::RemoveHead { todo_bytes } => {
                todo_bytes.saturating_add(1)
            }
            C3Work::OutputAppend {
                prefix_len,
                capacity,
            } if prefix_len == capacity => prefix_len.saturating_add(1),
            _ => 1,
        })
    }
}

impl<'db> ClassContextEffects<'db> for AttemptMroEffects<'db> {
    type Error = Incomplete;

    fn checkpoint(&self, work: ClassContextWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        self.admit(1)
    }

    fn is_version_info(&self, class: StaticClassLiteral<'db>) -> Result<bool, Incomplete> {
        Ok(class.is_known(self.db, KnownClass::VersionInfo))
    }

    fn pep695_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Incomplete> {
        read_source(self, || class.pep695_generic_context(self.db))
    }

    fn legacy_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Incomplete> {
        legacy_generic_context_with(class, self)
    }

    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Incomplete> {
        self.explicit_bases(class)
    }

    fn next_base(
        &self,
        cursor: &mut ClassContextBaseCursor<'db>,
    ) -> Result<Option<(usize, Type<'db>)>, Incomplete> {
        Ok(cursor.next_base())
    }

    fn inherited_legacy_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Incomplete> {
        read_source(self, || class.inherited_legacy_generic_context(self.db))
    }
}

impl<'db> InheritedContextEffects<'db> for AttemptMroEffects<'db> {
    type Error = Incomplete;

    fn checkpoint(&self, work: InheritedContextWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        self.admit(match work {
            InheritedContextWork::Variables { retained } => retained.saturating_add(1),
            InheritedContextWork::Intern { len } => len,
            _ => 1,
        })
    }

    fn definition(&self, class: StaticClassLiteral<'db>) -> Result<Definition<'db>, Incomplete> {
        read_source(self, || class.definition(self.db))
    }

    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Incomplete> {
        self.explicit_bases(class)
    }

    fn find_variables(
        &self,
        env: &ProgramEnvironment<'db>,
        definition: Definition<'db>,
        base: Type<'db>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<(), Incomplete> {
        find_legacy_typevars_with(self.db, env, base, Some(definition), variables, self)
    }

    fn build_context(
        &self,
        env: &ProgramEnvironment<'db>,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<GenericContext<'db>, Incomplete> {
        GenericContext::from_typevar_instances_with(self.db, env, variables, self)
    }
}

impl ContextConstructionControl for AttemptMroEffects<'_> {
    type Error = Incomplete;

    fn checkpoint(&self, work: ContextConstructionWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        self.admit(match work {
            ContextConstructionWork::InitialCapacity { lower_bound } => {
                lower_bound.saturating_add(1)
            }
            ContextConstructionWork::Insert { len, capacity } if len == capacity => {
                len.saturating_add(capacity).saturating_add(1)
            }
            ContextConstructionWork::Shrink { len, capacity } => {
                len.saturating_add(capacity).saturating_add(1)
            }
            ContextConstructionWork::Intern { len } => len.saturating_add(1),
            _ => 1,
        })
    }
}

impl<'db> LegacyTypeVarEffects<'db> for AttemptMroEffects<'db> {
    type Error = Incomplete;

    fn checkpoint(&self, work: LegacyTypeVarWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        self.admit(match work {
            LegacyTypeVarWork::Pending { len, capacity }
            | LegacyTypeVarWork::Insert { len, capacity }
                if len == capacity =>
            {
                len.saturating_add(capacity).saturating_add(1)
            }
            _ => 1,
        })
    }

    fn normalize_paramspec(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Incomplete> {
        read_source(self, || variable.without_paramspec_attr(db))
    }

    fn deferred(
        &self,
        _: &'db dyn Db,
        _: &ProgramEnvironment<'db>,
        _: Option<Definition<'db>>,
        _: LegacyTypeVarDependency<'db>,
        _: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        _: &FindLegacyTypeVarsVisitor<'db>,
    ) -> Result<(), Incomplete> {
        self.unsupported(UnsupportedMroOperation::InheritedContext)
    }
}

impl super::construction::sealed::Sealed for AttemptMroEffects<'_> {}

impl<'db> StaticMroFacts<'db> for AttemptMroEffects<'db> {
    type Error = Incomplete;
}

impl<'db> SynchronousStaticMroEffects<'db> for AttemptMroEffects<'db> {
    fn body_scope(&self, class: StaticClassLiteral<'db>) -> Result<ScopeId<'db>, Incomplete> {
        Ok(class.body_scope(self.db))
    }

    fn is_object(&self, class: ClassType<'db>) -> Result<bool, Incomplete> {
        Ok(crate::types::mro::field_reads::MroFieldReads::new(self.db).is_object(class))
    }

    fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Incomplete> {
        Ok(crate::types::mro::field_reads::MroFieldReads::new(self.db).static_class_literal(class))
    }

    fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&[Type<'db>], Incomplete> {
        self.explicit_bases(class)
    }

    fn has_pep_695_type_params(&self, class: StaticClassLiteral<'db>) -> Result<bool, Incomplete> {
        ClassContextEffects::pep695_generic_context(self, class).map(|context| context.is_some())
    }

    fn converted_explicit_base(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        _index: usize,
        ty: Type<'db>,
    ) -> Result<Option<ClassBase<'db>>, Incomplete> {
        self.check()?;
        let result = match ClassBaseConversion::from_explicit_type(ty) {
            ClassBaseConversion::Ready(base) => base,
            ClassBaseConversion::Dependency(
                dependency @ ClassBaseDependency::DefaultSpecialization(_),
            ) => read_source(self, || {
                dependency.resolve(self.db, env, Some(class.into()))
            })?,
            ClassBaseConversion::Dependency(_) => {
                return self.unsupported(UnsupportedMroOperation::BaseConversion);
            }
        };
        self.check()?;
        Ok(result)
    }

    fn object_base(&self, env: &ProgramEnvironment<'db>) -> Result<ClassBase<'db>, Incomplete> {
        self.object_base(env)
    }

    fn checkpoint(&self, work: StaticMroWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        let units = match work {
            StaticMroWork::GenericProtocolScan { len }
            | StaticMroWork::GenericAliasScan { len }
            | StaticMroWork::InvalidBasesBox { len, .. }
            | StaticMroWork::DirectSequenceCapacity { len } => len,
            StaticMroWork::FixedMro { entries } => entries,
            StaticMroWork::SequenceStart { bases } => bases.saturating_add(3),
            StaticMroWork::ResolvedBaseAppend { prefix_len, .. }
            | StaticMroWork::InvalidBaseAppend { prefix_len, .. }
            | StaticMroWork::SequenceAppend { prefix_len, .. } => prefix_len.saturating_add(1),
            _ => 1,
        };
        self.admit(units)
    }

    fn root_class(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassType<'db>, Incomplete> {
        apply_optional_class_specialization_sync(self.db, class, specialization, self)
    }

    fn static_mro_is_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<bool, Incomplete> {
        Ok(
            read_source(self, || class.try_mro(self.db, specialization))?
                .is_err_and(StaticMroError::is_cycle),
        )
    }

    fn collect_single_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        root: ClassType<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<Mro<'db>, Incomplete> {
        collect_single_base_mro_sync(self.db, env, root, base, additional, self)
    }

    fn collect_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<VecDeque<ClassBase<'db>>, Incomplete> {
        collect_base_mro_sync(self.db, env, base, additional, self)
    }

    fn specialize_base(
        &self,
        base: ClassBase<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassBase<'db>, Incomplete> {
        crate::types::mapping::attempt::specialize_base(self.db, base, specialization)
    }

    fn c3_merge(
        &self,
        sequences: Vec<VecDeque<ClassBase<'db>>>,
    ) -> Result<Option<Mro<'db>>, Incomplete> {
        c3_merge_sync(self.db, sequences, self)
    }

    fn make_error(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        kind: StaticMroErrorKind<'db>,
    ) -> Result<StaticMroError<'db>, Incomplete> {
        let object = self.object_base(env)?;
        self.admit(3)?;
        #[cfg(feature = "experimental-analysis")]
        if let StaticMroErrorKind::DuplicateBases(bases) = &kind {
            self.admit(bases.len())?;
        }
        Ok(kind.into_mro_error_with_object(class, object))
    }

    fn failed_c3(
        &self,
        _: &ProgramEnvironment<'db>,
        _: StaticClassLiteral<'db>,
        _: ClassType<'db>,
        _: &[Type<'db>],
        _: &[ClassBase<'db>],
    ) -> Result<Result<Mro<'db>, StaticMroError<'db>>, Incomplete> {
        self.unsupported(UnsupportedMroOperation::ErrorDetails)
    }
}
