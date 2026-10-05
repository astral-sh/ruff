//! Shared protocol candidate reduction and interface assembly.

use std::collections::BTreeMap;
use std::convert::Infallible;

#[cfg(test)]
pub(in crate::types) mod runtime;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashMap;
use ty_python_core::definition::Definition;
use ty_python_core::place::PlaceTable;
use ty_python_core::scope::ScopeId;
use ty_python_core::{
    BindingWithConstraintsIterator, DeclarationsIterator, ImportedFinalCandidatesIterator,
    UseDefMap, place_table, use_def_map,
};

use super::{
    BoundOnClass, ProtocolMemberCandidate, ProtocolMemberData, ProtocolMemberType,
    ProtocolMemberWrite, descriptor_decorated_protocol_member, excluded_from_proto_members,
};
use crate::place::{
    PlaceFromDeclarationsResult, PlaceWithDefinition, place_from_bindings, place_from_declarations,
};
use crate::types::generics::Specialization;
use crate::types::mro::base::{InlineBaseMroEffects, class_mro_start_sync};
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_sync};
use crate::types::mro::root::InlineMroRootEffects;
use crate::types::{
    CallableType, ClassBase, ClassType, FunctionType, PropertyInstanceType, Type, TypeQualifiers,
};
use crate::{Db, ProgramEnvironment};

/// Progress boundaries and retained data inspected by native collection operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ProtocolInterfaceWork {
    Build,
    MroAdvance,
    BindingAdvance,
    Bindings {
        entries: usize,
    },
    DeclarationAdvance,
    Declarations {
        entries: usize,
        imported_entries: usize,
    },
    CandidateAdvance,
    CandidateName {
        bytes: usize,
    },
    MemberLookup {
        name_bytes: usize,
    },
    MemberInsert {
        name_bytes: usize,
    },
    Complete,
}

pub(in crate::types) struct ProtocolInterfaceBuild<'db> {
    class: ClassType<'db>,
    members: BTreeMap<Name, ProtocolMemberData<'db>>,
}

pub(in crate::types) struct PreparedProtocolInterface<'db> {
    pub(in crate::types) env: ProgramEnvironment<'db>,
    pub(in crate::types) members: BTreeMap<Name, ProtocolMemberData<'db>>,
}

pub(in crate::types) trait ProtocolCandidateEffects<'db, C> {
    type Error;

    async fn mro_start(&self, class: ClassType<'db>) -> Result<MroCursor<'db>, Self::Error>;
    async fn mro_next(
        &self,
        cursor: &mut MroCursor<'db>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error>;
    async fn protocol_scope(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(ScopeId<'db>, Option<Specialization<'db>>)>, Self::Error>;
    async fn use_def_map(&self, scope: ScopeId<'db>) -> Result<&'db UseDefMap<'db>, Self::Error>;
    async fn place_table(&self, scope: ScopeId<'db>) -> Result<&'db PlaceTable, Self::Error>;
    async fn binding_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        bindings: BindingWithConstraintsIterator<'map, 'db>,
    ) -> Result<PlaceWithDefinition<'db>, Self::Error>;
    async fn declaration_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'map, 'db>,
        imported: ImportedFinalCandidatesIterator<'map, 'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error>;
    async fn checkpoint(&self, work: ProtocolInterfaceWork) -> Result<(), Self::Error>;
    async fn visit_candidate(
        &self,
        env: &ProgramEnvironment<'db>,
        consumer: &mut C,
        name: &Name,
        candidate: ProtocolMemberCandidate<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<(), Self::Error>;
}

pub(in crate::types) trait SyncProtocolCandidateEffects<'db, C> {
    type Error;

    fn mro_start(&self, class: ClassType<'db>) -> Result<MroCursor<'db>, Self::Error>;
    fn mro_next(&self, cursor: &mut MroCursor<'db>) -> Result<Option<ClassBase<'db>>, Self::Error>;
    fn protocol_scope(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(ScopeId<'db>, Option<Specialization<'db>>)>, Self::Error>;
    fn use_def_map(&self, scope: ScopeId<'db>) -> Result<&'db UseDefMap<'db>, Self::Error>;
    fn place_table(&self, scope: ScopeId<'db>) -> Result<&'db PlaceTable, Self::Error>;
    fn binding_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        bindings: BindingWithConstraintsIterator<'map, 'db>,
    ) -> Result<PlaceWithDefinition<'db>, Self::Error>;
    fn declaration_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'map, 'db>,
        imported: ImportedFinalCandidatesIterator<'map, 'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error>;
    fn checkpoint(&self, work: ProtocolInterfaceWork) -> Result<(), Self::Error>;
    fn visit_candidate(
        &self,
        env: &ProgramEnvironment<'db>,
        consumer: &mut C,
        name: &Name,
        candidate: ProtocolMemberCandidate<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<(), Self::Error>;
}

pub(in crate::types) trait ProtocolInterfaceEffects<'db>:
    ProtocolCandidateEffects<'db, ProtocolInterfaceBuild<'db>>
{
    async fn environment(
        &self,
        class: ClassType<'db>,
    ) -> Result<ProgramEnvironment<'db>, Self::Error>;
    async fn with_typevar_bounds(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error>;
    async fn specialize_candidate_type(
        &self,
        ty: Type<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Type<'db>, Self::Error>;
    async fn property_accessors(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> Result<(Option<Type<'db>>, Option<Type<'db>>), Self::Error>;
    async fn callable_is_method_like(
        &self,
        callable: CallableType<'db>,
    ) -> Result<bool, Self::Error>;
    async fn function_is_staticmethod(
        &self,
        function: FunctionType<'db>,
    ) -> Result<bool, Self::Error>;
    async fn function_is_classmethod(
        &self,
        function: FunctionType<'db>,
    ) -> Result<bool, Self::Error>;
    async fn function_callable(
        &self,
        function: FunctionType<'db>,
    ) -> Result<CallableType<'db>, Self::Error>;
    async fn definition_is_function(
        &self,
        definition: Definition<'db>,
    ) -> Result<bool, Self::Error>;
    async fn method_member(
        &self,
        env: &ProgramEnvironment<'db>,
        callable: CallableType<'db>,
        definition: Option<Definition<'db>>,
    ) -> Result<ProtocolMemberData<'db>, Self::Error>;
    async fn descriptor_member(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        class: ClassType<'db>,
        definition: Option<Definition<'db>>,
    ) -> Result<Option<ProtocolMemberData<'db>>, Self::Error>;
}

pub(in crate::types) trait SyncProtocolInterfaceEffects<'db>:
    SyncProtocolCandidateEffects<'db, ProtocolInterfaceBuild<'db>>
{
    fn environment(&self, class: ClassType<'db>) -> Result<ProgramEnvironment<'db>, Self::Error>;
    fn with_typevar_bounds(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error>;
    fn specialize_candidate_type(
        &self,
        ty: Type<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Type<'db>, Self::Error>;
    fn property_accessors(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> Result<(Option<Type<'db>>, Option<Type<'db>>), Self::Error>;
    fn callable_is_method_like(&self, callable: CallableType<'db>) -> Result<bool, Self::Error>;
    fn function_is_staticmethod(&self, function: FunctionType<'db>) -> Result<bool, Self::Error>;
    fn function_is_classmethod(&self, function: FunctionType<'db>) -> Result<bool, Self::Error>;
    fn function_callable(
        &self,
        function: FunctionType<'db>,
    ) -> Result<CallableType<'db>, Self::Error>;
    fn definition_is_function(&self, definition: Definition<'db>) -> Result<bool, Self::Error>;
    fn method_member(
        &self,
        env: &ProgramEnvironment<'db>,
        callable: CallableType<'db>,
        definition: Option<Definition<'db>>,
    ) -> Result<ProtocolMemberData<'db>, Self::Error>;
    fn descriptor_member(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        class: ClassType<'db>,
        definition: Option<Definition<'db>>,
    ) -> Result<Option<ProtocolMemberData<'db>>, Self::Error>;
}

#[ty_mapping_probe_macros::dual_protocol_interface]
pub(in crate::types) async fn for_each_protocol_member_candidate_with<
    'db,
    C,
    E: ProtocolCandidateEffects<'db, C>,
>(
    class: ClassType<'db>,
    env: &ProgramEnvironment<'db>,
    consumer: &mut C,
    effects: &E,
) -> Result<(), E::Error> {
    let mut mro = effects.mro_start(class).await?;
    loop {
        effects
            .checkpoint(ProtocolInterfaceWork::MroAdvance)
            .await?;
        let Some(base) = effects.mro_next(&mut mro).await? else {
            break;
        };
        let Some(class) = base.into_class() else {
            continue;
        };
        let Some((parent_scope, specialization)) = effects.protocol_scope(class).await? else {
            continue;
        };
        let use_def_map = effects.use_def_map(parent_scope).await?;
        let place_table = effects.place_table(parent_scope).await?;
        let mut direct_members = FxHashMap::default();

        // Bindings that are not declared in the class body are invalid protocol members, but
        // runtime-checkable protocols still consider them members for `isinstance()` and
        // `issubclass()`.
        let mut bindings = use_def_map.all_end_of_scope_symbol_bindings();
        loop {
            effects
                .checkpoint(ProtocolInterfaceWork::BindingAdvance)
                .await?;
            let Some((symbol_id, bindings)) = bindings.next() else {
                break;
            };
            effects
                .checkpoint(ProtocolInterfaceWork::Bindings {
                    entries: bindings.traversal_len(),
                })
                .await?;
            let place_and_definition = effects.binding_place(env, bindings).await?;
            if let Some(ty) = place_and_definition.place.ignore_possibly_undefined() {
                direct_members.insert(
                    symbol_id,
                    ProtocolMemberCandidate {
                        ty,
                        qualifiers: TypeQualifiers::default(),
                        definition: place_and_definition.first_definition,
                        bound_on_class: BoundOnClass::Yes,
                    },
                );
            }
        }

        let mut declarations = use_def_map.all_end_of_scope_symbol_declarations();
        loop {
            effects
                .checkpoint(ProtocolInterfaceWork::DeclarationAdvance)
                .await?;
            let Some((symbol_id, declarations)) = declarations.next() else {
                break;
            };
            let imported = use_def_map.end_of_scope_imported_final_candidates(symbol_id.into());
            effects
                .checkpoint(ProtocolInterfaceWork::Declarations {
                    entries: declarations.traversal_len(),
                    imported_entries: imported.traversal_len(),
                })
                .await?;
            let place_result = effects
                .declaration_place(env, declarations, imported)
                .await?;
            let first_declaration = place_result.first_declaration;
            let place = place_result.ignore_conflicting_declarations();
            if let Some(ty) = place.place.ignore_possibly_undefined() {
                direct_members
                    .entry(symbol_id)
                    .and_modify(|candidate| {
                        candidate.ty = ty;
                        candidate.qualifiers = place.qualifiers;
                    })
                    .or_insert(ProtocolMemberCandidate {
                        ty,
                        qualifiers: place.qualifiers,
                        definition: first_declaration,
                        bound_on_class: BoundOnClass::No,
                    });
            }
        }

        #[expect(
            clippy::iter_over_hash_type,
            reason = "member names are unique within each class and both consumers are order-independent"
        )]
        let mut candidates = direct_members.into_iter();
        loop {
            effects
                .checkpoint(ProtocolInterfaceWork::CandidateAdvance)
                .await?;
            let Some((symbol_id, candidate)) = candidates.next() else {
                break;
            };
            let name = place_table.symbol(symbol_id).name();
            effects
                .checkpoint(ProtocolInterfaceWork::CandidateName { bytes: name.len() })
                .await?;
            if excluded_from_proto_members(name) {
                continue;
            }
            effects
                .visit_candidate(env, consumer, name, candidate, specialization)
                .await?;
        }
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_protocol_interface]
pub(in crate::types) async fn protocol_interface_candidate_with<
    'db,
    E: ProtocolInterfaceEffects<'db>,
>(
    env: &ProgramEnvironment<'db>,
    build: &mut ProtocolInterfaceBuild<'db>,
    name: &Name,
    candidate: ProtocolMemberCandidate<'db>,
    specialization: Option<Specialization<'db>>,
    effects: &E,
) -> Result<(), E::Error> {
    effects
        .checkpoint(ProtocolInterfaceWork::MemberLookup {
            name_bytes: name.len(),
        })
        .await?;
    if build.members.contains_key(name) {
        return Ok(());
    }

    let specialization = if let Some(specialization) = specialization {
        Some(effects.with_typevar_bounds(specialization).await?)
    } else {
        None
    };
    let ty = effects
        .specialize_candidate_type(candidate.ty, specialization)
        .await?;
    let ProtocolMemberCandidate {
        qualifiers,
        definition,
        bound_on_class,
        ..
    } = candidate;
    let member = match ty {
        Type::PropertyInstance(property) => {
            let (getter, setter) = effects.property_accessors(property).await?;
            ProtocolMemberData::property(
                getter.map(ProtocolMemberType::property_getter),
                setter
                    .map(ProtocolMemberType::property_setter)
                    .map(ProtocolMemberWrite::from_type),
                definition,
            )
        }
        Type::Callable(callable)
            if bound_on_class.is_yes() && effects.callable_is_method_like(callable).await? =>
        {
            effects.method_member(env, callable, definition).await?
        }
        Type::FunctionLiteral(function)
            if bound_on_class.is_yes()
                || effects.function_is_staticmethod(function).await?
                || effects.function_is_classmethod(function).await? =>
        {
            let callable = effects.function_callable(function).await?;
            effects.method_member(env, callable, definition).await?
        }
        _ if bound_on_class.is_yes()
            && match definition {
                Some(definition) => effects.definition_is_function(definition).await?,
                None => false,
            } =>
        {
            if let Some(descriptor) = effects
                .descriptor_member(env, ty, build.class, definition)
                .await?
            {
                descriptor
            } else {
                ProtocolMemberData::attribute(ty, qualifiers, definition)
            }
        }
        _ => ProtocolMemberData::attribute(ty, qualifiers, definition),
    };

    effects
        .checkpoint(ProtocolInterfaceWork::MemberInsert {
            name_bytes: name.len(),
        })
        .await?;
    build.members.insert(name.clone(), member);
    Ok(())
}

#[ty_mapping_probe_macros::dual_protocol_interface]
pub(in crate::types) async fn protocol_interface_build_with<
    'db,
    E: ProtocolInterfaceEffects<'db>,
>(
    class: ClassType<'db>,
    effects: &E,
) -> Result<PreparedProtocolInterface<'db>, E::Error> {
    effects.checkpoint(ProtocolInterfaceWork::Build).await?;
    let env = effects.environment(class).await?;
    let mut build = ProtocolInterfaceBuild {
        class,
        members: BTreeMap::default(),
    };
    for_each_protocol_member_candidate_with(class, &env, &mut build, effects).await?;
    effects.checkpoint(ProtocolInterfaceWork::Complete).await?;
    Ok(PreparedProtocolInterface {
        env,
        members: build.members,
    })
}

pub(super) struct CallbackConsumer<F>(pub(super) F);

pub(super) struct InlineProtocolInterfaceEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> InlineProtocolInterfaceEffects<'db> {
    pub(super) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

// Both ordinary consumers use the same source reductions; only candidate delivery differs.
macro_rules! inline_candidate_requests {
    () => {
        type Error = Infallible;

        #[inline]
        fn mro_start(&self, class: ClassType<'db>) -> Result<MroCursor<'db>, Infallible> {
            let start =
                class_mro_start_sync(self.db, class, None, &InlineBaseMroEffects::new(self.db))?;
            Ok(MroCursor::new(start.class, start.specialization))
        }

        #[inline]
        fn mro_next(
            &self,
            cursor: &mut MroCursor<'db>,
        ) -> Result<Option<ClassBase<'db>>, Infallible> {
            mro_next_sync(
                self.db,
                cursor,
                MroDirection::Forward,
                &InlineMroRootEffects::new(self.db),
            )
        }

        #[inline]
        fn protocol_scope(
            &self,
            class: ClassType<'db>,
        ) -> Result<Option<(ScopeId<'db>, Option<Specialization<'db>>)>, Infallible> {
            let Some((class_literal, specialization)) = class.static_class_literal(self.db) else {
                return Ok(None);
            };
            let Some(protocol_class) = class_literal.into_protocol_class(self.db) else {
                return Ok(None);
            };
            Ok(protocol_class
                .static_class_literal(self.db)
                .map(|(class_literal, _)| (class_literal.body_scope(self.db), specialization)))
        }

        #[inline]
        fn use_def_map(&self, scope: ScopeId<'db>) -> Result<&'db UseDefMap<'db>, Infallible> {
            Ok(use_def_map(self.db, scope))
        }

        #[inline]
        fn place_table(&self, scope: ScopeId<'db>) -> Result<&'db PlaceTable, Infallible> {
            Ok(place_table(self.db, scope))
        }

        #[inline]
        fn binding_place<'map>(
            &self,
            env: &ProgramEnvironment<'db>,
            bindings: BindingWithConstraintsIterator<'map, 'db>,
        ) -> Result<PlaceWithDefinition<'db>, Infallible> {
            Ok(place_from_bindings(self.db, env, bindings))
        }

        #[inline]
        fn declaration_place<'map>(
            &self,
            env: &ProgramEnvironment<'db>,
            declarations: DeclarationsIterator<'map, 'db>,
            imported: ImportedFinalCandidatesIterator<'map, 'db>,
        ) -> Result<PlaceFromDeclarationsResult<'db>, Infallible> {
            Ok(place_from_declarations(self.db, env, declarations)
                .with_imported_final(self.db, env, imported))
        }

        #[inline]
        fn checkpoint(&self, _work: ProtocolInterfaceWork) -> Result<(), Infallible> {
            Ok(())
        }
    };
}

impl<'db, F> SyncProtocolCandidateEffects<'db, CallbackConsumer<F>>
    for InlineProtocolInterfaceEffects<'db>
where
    F: FnMut(&Name, ProtocolMemberCandidate<'db>, Option<Specialization<'db>>),
{
    inline_candidate_requests!();

    #[inline]
    fn visit_candidate(
        &self,
        _env: &ProgramEnvironment<'db>,
        consumer: &mut CallbackConsumer<F>,
        name: &Name,
        candidate: ProtocolMemberCandidate<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<(), Infallible> {
        (consumer.0)(name, candidate, specialization);
        Ok(())
    }
}

impl<'db> SyncProtocolCandidateEffects<'db, ProtocolInterfaceBuild<'db>>
    for InlineProtocolInterfaceEffects<'db>
{
    inline_candidate_requests!();

    #[inline]
    fn visit_candidate(
        &self,
        env: &ProgramEnvironment<'db>,
        consumer: &mut ProtocolInterfaceBuild<'db>,
        name: &Name,
        candidate: ProtocolMemberCandidate<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<(), Infallible> {
        protocol_interface_candidate_sync(env, consumer, name, candidate, specialization, self)
    }
}

impl<'db> SyncProtocolInterfaceEffects<'db> for InlineProtocolInterfaceEffects<'db> {
    #[inline]
    fn environment(&self, class: ClassType<'db>) -> Result<ProgramEnvironment<'db>, Infallible> {
        Ok(ProgramEnvironment::from_file(
            class.class_literal(self.db).program_file(self.db),
        ))
    }

    #[inline]
    fn with_typevar_bounds(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Infallible> {
        Ok(specialization.with_typevar_bounds(self.db))
    }

    #[inline]
    fn specialize_candidate_type(
        &self,
        ty: Type<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(ty.apply_optional_specialization(self.db, specialization))
    }

    #[inline]
    fn property_accessors(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> Result<(Option<Type<'db>>, Option<Type<'db>>), Infallible> {
        Ok((property.getter(self.db), property.setter(self.db)))
    }

    #[inline]
    fn callable_is_method_like(&self, callable: CallableType<'db>) -> Result<bool, Infallible> {
        Ok(callable.is_method_like(self.db))
    }

    #[inline]
    fn function_is_staticmethod(&self, function: FunctionType<'db>) -> Result<bool, Infallible> {
        Ok(function.is_staticmethod(self.db))
    }

    #[inline]
    fn function_is_classmethod(&self, function: FunctionType<'db>) -> Result<bool, Infallible> {
        Ok(function.is_classmethod(self.db))
    }

    #[inline]
    fn function_callable(
        &self,
        function: FunctionType<'db>,
    ) -> Result<CallableType<'db>, Infallible> {
        Ok(function.into_callable_type(self.db))
    }

    #[inline]
    fn definition_is_function(&self, definition: Definition<'db>) -> Result<bool, Infallible> {
        Ok(definition.kind(self.db).is_function_def())
    }

    #[inline]
    fn method_member(
        &self,
        env: &ProgramEnvironment<'db>,
        callable: CallableType<'db>,
        definition: Option<Definition<'db>>,
    ) -> Result<ProtocolMemberData<'db>, Infallible> {
        Ok(ProtocolMemberData::method(
            self.db, env, callable, definition,
        ))
    }

    #[inline]
    fn descriptor_member(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        class: ClassType<'db>,
        definition: Option<Definition<'db>>,
    ) -> Result<Option<ProtocolMemberData<'db>>, Infallible> {
        Ok(descriptor_decorated_protocol_member(
            self.db, env, ty, class, definition,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::types) enum ProtocolNormalizationWork {
    Start,
    Advance,
    Member { name_bytes: usize },
    Complete,
}

pub(in crate::types) trait ProtocolInterfaceNormalizationEffects<'db> {
    type Error;
    async fn checkpoint(&self, work: ProtocolNormalizationWork) -> Result<(), Self::Error>;
    async fn normalize_member(
        &self,
        env: &ProgramEnvironment<'db>,
        current: &ProtocolMemberData<'db>,
        previous: &ProtocolMemberData<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> Result<ProtocolMemberData<'db>, Self::Error>;
}

pub(in crate::types) trait SyncProtocolInterfaceNormalizationEffects<'db> {
    type Error;
    fn checkpoint(&self, work: ProtocolNormalizationWork) -> Result<(), Self::Error>;
    fn normalize_member(
        &self,
        env: &ProgramEnvironment<'db>,
        current: &ProtocolMemberData<'db>,
        previous: &ProtocolMemberData<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> Result<ProtocolMemberData<'db>, Self::Error>;
}

#[ty_mapping_probe_macros::dual_protocol_interface]
pub(in crate::types) async fn protocol_interface_normalize_with<
    'db,
    E: ProtocolInterfaceNormalizationEffects<'db>,
>(
    env: &ProgramEnvironment<'db>,
    previous: &BTreeMap<Name, ProtocolMemberData<'db>>,
    current: &BTreeMap<Name, ProtocolMemberData<'db>>,
    cycle: &salsa::Cycle<'_>,
    effects: &E,
) -> Result<BTreeMap<Name, ProtocolMemberData<'db>>, E::Error> {
    effects.checkpoint(ProtocolNormalizationWork::Start).await?;
    let mut members = BTreeMap::new();
    let mut current = current.iter();
    loop {
        effects
            .checkpoint(ProtocolNormalizationWork::Advance)
            .await?;
        let Some((name, data)) = current.next() else {
            break;
        };
        effects
            .checkpoint(ProtocolNormalizationWork::Member {
                name_bytes: name.len(),
            })
            .await?;
        let normalized = if let Some(previous) = previous.get(name) {
            effects.normalize_member(env, data, previous, cycle).await?
        } else {
            data.clone()
        };
        members.insert(name.clone(), normalized);
    }
    effects
        .checkpoint(ProtocolNormalizationWork::Complete)
        .await?;
    Ok(members)
}

impl<'db> SyncProtocolInterfaceNormalizationEffects<'db> for InlineProtocolInterfaceEffects<'db> {
    type Error = Infallible;
    fn checkpoint(&self, _work: ProtocolNormalizationWork) -> Result<(), Infallible> {
        Ok(())
    }
    fn normalize_member(
        &self,
        env: &ProgramEnvironment<'db>,
        current: &ProtocolMemberData<'db>,
        previous: &ProtocolMemberData<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> Result<ProtocolMemberData<'db>, Infallible> {
        Ok(current.cycle_normalized(self.db, env, previous, cycle))
    }
}
