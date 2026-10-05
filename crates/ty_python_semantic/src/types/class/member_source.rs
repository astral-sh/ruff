//! Shared declaration-source reductions used by class and instance member lookup.

use std::convert::Infallible;

use ruff_python_ast::name::Name;
use ty_python_core::{
    BindingWithConstraintsIterator, DeclarationsIterator, ImportedFinalCandidatesIterator,
    PlaceTable, UseDefMap, place_table, scope::ScopeId, symbol::ScopedSymbolId, use_def_map,
};

use super::implicit_attributes::ImplicitAttribute;
use super::{CodeGeneratorKind, MethodDecorator, StaticClassLiteral};
use crate::place::{
    ConsideredDefinitions, DefinedPlace, Definedness, Place, PlaceAndQualifiers,
    PlaceFromDeclarationsResult, PlaceWithDefinition, Provenance, PublicTypePolicy,
    RequiresExplicitReExport, TypeOrigin, place_by_id, place_from_bindings,
};
use crate::types::member::Member;
use crate::types::{DataclassParams, KnownClass, Type, TypeQualifiers};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum MemberSourceWork {
    Begin,
    ClassHeader,
    PlaceTable,
    Symbol,
    UseDef,
    Declarations,
    ImportedFinal,
    Bindings,
    Classify,
    ImplicitNames,
    ImplicitSearch,
    ImplicitInference,
    CodeGenerator,
    NamedTupleField,
    KwOnly,
    Stub,
    InstanceSlot,
    DataclassField,
    Getter,
    Union,
    Publish,
}

/// Selects the policy that requires own-field lookup to identify an instance attribute.
/// `None` means the class's code generator does not supply such instance fields.
pub(in crate::types) fn instance_field_policy<'db>(
    generator: Option<CodeGeneratorKind<'db>>,
) -> Option<CodeGeneratorKind<'db>> {
    generator.filter(|policy| policy.treats_fields_as_instance_attributes())
}

/// Finite operations on retained source values; cursor advancement belongs to the reducer.
#[derive(Clone, Copy)]
pub(in crate::types) struct RawClassMemberFacts;

ty_mapping_probe_macros::shared_semantic_family! {
#[synchronous(SynchronousMemberSourceEffects)]
pub(in crate::types) trait MemberSourceEffects<'db>: sealed::Sealed {
    type Error;
    #[operation(checkpoint)]
    async fn checkpoint(&self, work: MemberSourceWork) -> Result<(), Self::Error>;
    #[operation(source)]
    async fn place_table(&self, scope: ScopeId<'db>) -> Result<&'db PlaceTable, Self::Error>;
    #[operation(source)]
    async fn use_def_map(&self, scope: ScopeId<'db>) -> Result<&'db UseDefMap<'db>, Self::Error>;
    #[operation(local)]
    async fn symbol_id(
        &self,
        table: &'db PlaceTable,
        name: &str,
    ) -> Result<Option<ScopedSymbolId>, Self::Error>;
    #[operation(child)]
    async fn binding_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        bindings: BindingWithConstraintsIterator<'map, 'db>,
    ) -> Result<PlaceWithDefinition<'db>, Self::Error>;
}
#[synchronous(SynchronousRawClassMemberEffects)]
pub(in crate::types) trait RawClassMemberEffects<'db>:
    MemberSourceEffects<'db>
{
    #[operation(child)]
    async fn public_class_place(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
}

#[finite_capability]
impl RawClassMemberFacts {
    fn bindings<'map, 'db>(
        &self,
        use_def: &'map UseDefMap<'db>,
        symbol: ScopedSymbolId,
    ) -> BindingWithConstraintsIterator<'map, 'db> {
        use_def.end_of_scope_symbol_bindings(symbol)
    }

    fn environment<'db>(&self, scope: ScopeId<'db>) -> ProgramEnvironment<'db> {
        ProgramEnvironment::from_scope(scope)
    }

    fn is_undefined(&self, place: PlaceAndQualifiers<'_>) -> bool {
        place.is_undefined()
    }

    fn is_init_var(&self, place: PlaceAndQualifiers<'_>) -> bool {
        place.is_init_var()
    }

    fn with_qualifiers<'db>(
        &self,
        place: Place<'db>,
        qualifiers: TypeQualifiers,
    ) -> PlaceAndQualifiers<'db> {
        place.with_qualifiers(qualifiers)
    }

    fn provenance<'db>(&self, first: Provenance<'db>, second: Provenance<'db>) -> Provenance<'db> {
        first.or(second)
    }

    fn unbound<'db>(&self) -> Member<'db> {
        Member::unbound()
    }
}

#[synchronous(raw_class_member_sync)]
#[capabilities(effects = RawClassMemberEffects, facts = RawClassMemberFacts)]
#[passive_values(Member, Place::Defined, Place::Undefined, DefinedPlace, PlaceAndQualifiers, MemberSourceWork)]
pub(in crate::types) async fn raw_class_member_with<'a, 'db, E: RawClassMemberEffects<'db>>(
    scope: ScopeId<'db>,
    name: &'a str,
    facts: RawClassMemberFacts,
    effects: &E,
) -> Result<Member<'db>, E::Error> {
    effects.checkpoint(MemberSourceWork::Begin).await?;
    effects.checkpoint(MemberSourceWork::PlaceTable).await?;
    let table = effects.place_table(scope).await?;
    effects.checkpoint(MemberSourceWork::Symbol).await?;
    let Some(symbol_id) = effects.symbol_id(table, name).await? else {
        effects.checkpoint(MemberSourceWork::Publish).await?;
        return Ok(facts.unbound());
    };
    effects.checkpoint(MemberSourceWork::Declarations).await?;
    let place_and_quals = effects.public_class_place(scope, symbol_id).await?;
    effects.checkpoint(MemberSourceWork::Classify).await?;
    if !facts.is_undefined(place_and_quals) && !facts.is_init_var(place_and_quals) {
        // Trust the declared type if we see a class-level declaration
        effects.checkpoint(MemberSourceWork::Publish).await?;
        return Ok(Member {
            inner: place_and_quals,
        });
    }

    let member = if let PlaceAndQualifiers {
        place:
            Place::Defined(DefinedPlace {
                ty,
                provenance: declared_provenance,
                ..
            }),
        qualifiers,
    } = place_and_quals
    {
        // Otherwise, we need to check if the symbol has bindings
        effects.checkpoint(MemberSourceWork::UseDef).await?;
        let use_def = effects.use_def_map(scope).await?;
        let bindings = facts.bindings(use_def, symbol_id);
        let env = facts.environment(scope);
        effects.checkpoint(MemberSourceWork::Bindings).await?;
        let inferred = effects.binding_place(&env, bindings).await?.place;

        // TODO: we should not need to calculate inferred type second time. This is a temporary
        // solution until the notion of Boundness and Declaredness is split. See #16036, #16264
        Member {
            inner: match inferred {
                Place::Undefined => facts.with_qualifiers(Place::Undefined, qualifiers),
                Place::Defined(place) => facts.with_qualifiers(
                    Place::Defined(DefinedPlace {
                        ty,
                        provenance: facts.provenance(place.provenance, declared_provenance),
                        ..place
                    }),
                    qualifiers,
                ),
            },
        }
    } else {
        facts.unbound()
    };
    effects.checkpoint(MemberSourceWork::Publish).await?;
    Ok(member)
}
}

pub(in crate::types) trait StaticInstanceMemberEffects<'db>:
    MemberSourceEffects<'db>
{
    async fn body_scope(&self, class: StaticClassLiteral<'db>)
    -> Result<ScopeId<'db>, Self::Error>;
    async fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error>;
    async fn has_own_named_tuple_field(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;
    async fn declaration_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'map, 'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error>;
    async fn imported_final<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        result: PlaceFromDeclarationsResult<'db>,
        imported: ImportedFinalCandidatesIterator<'map, 'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error>;
    async fn implicit_member(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;
    async fn is_kw_only(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
    async fn is_stub(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    async fn has_instance_slot(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;
    async fn is_own_dataclass_instance_field(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;
    async fn getter_member(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn union_two(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;
}
pub(in crate::types) trait ImplicitAttributeEffects<'db>: sealed::Sealed {
    async fn body_scope(&self, class: StaticClassLiteral<'db>)
    -> Result<ScopeId<'db>, Self::Error>;
    type Error;
    async fn checkpoint(&self, work: MemberSourceWork) -> Result<(), Self::Error>;
    async fn names(&self, scope: ScopeId<'db>) -> Result<&'db [Name], Self::Error>;
    async fn find_name(&self, names: &'db [Name], name: &str)
    -> Result<Option<usize>, Self::Error>;
    async fn infer_named_attribute(
        &self,
        scope: ScopeId<'db>,
        name: &'db Name,
        target: MethodDecorator,
    ) -> Result<ImplicitAttribute<'db>, Self::Error>;
}
pub(in crate::types) trait StaticCodeGeneratorEffects<'db>:
    sealed::Sealed
{
    async fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<DataclassParams<'db>>, Self::Error>;
    async fn known(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Self::Error>;
    async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>)
    -> Result<bool, Self::Error>;
    async fn has_explicit_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    type Error;
    async fn checkpoint(&self, work: MemberSourceWork) -> Result<(), Self::Error>;
    async fn code_generator_query(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error>;
}

pub(in crate::types) trait SynchronousStaticInstanceMemberEffects<'db>:
    SynchronousMemberSourceEffects<'db>
{
    fn body_scope(&self, class: StaticClassLiteral<'db>) -> Result<ScopeId<'db>, Self::Error>;
    fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error>;
    fn has_own_named_tuple_field(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;
    fn declaration_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'map, 'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error>;
    fn imported_final<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        result: PlaceFromDeclarationsResult<'db>,
        imported: ImportedFinalCandidatesIterator<'map, 'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error>;
    fn implicit_member(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;
    fn is_kw_only(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
    fn is_stub(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    fn has_instance_slot(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;
    fn is_own_dataclass_instance_field(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;
    fn getter_member(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    fn union_two(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;
}
pub(in crate::types) trait SynchronousImplicitAttributeEffects<'db>:
    sealed::Sealed
{
    fn body_scope(&self, class: StaticClassLiteral<'db>) -> Result<ScopeId<'db>, Self::Error>;
    type Error;
    fn checkpoint(&self, work: MemberSourceWork) -> Result<(), Self::Error>;
    fn names(&self, scope: ScopeId<'db>) -> Result<&'db [Name], Self::Error>;
    fn find_name(&self, names: &'db [Name], name: &str) -> Result<Option<usize>, Self::Error>;
    fn infer_named_attribute(
        &self,
        scope: ScopeId<'db>,
        name: &'db Name,
        target: MethodDecorator,
    ) -> Result<ImplicitAttribute<'db>, Self::Error>;
}
pub(in crate::types) trait SynchronousStaticCodeGeneratorEffects<'db>:
    sealed::Sealed
{
    fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<DataclassParams<'db>>, Self::Error>;
    fn known(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
    fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    fn has_explicit_metaclass(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;

    type Error;
    fn checkpoint(&self, work: MemberSourceWork) -> Result<(), Self::Error>;
    fn code_generator_query(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error>;
}

#[ty_mapping_probe_macros::dual_member_source]
pub(in crate::types) async fn static_own_instance_member_with<
    'a,
    'db,
    E: StaticInstanceMemberEffects<'db>,
>(
    env: &ProgramEnvironment<'db>,
    class: StaticClassLiteral<'db>,
    name: &'a str,
    effects: &E,
) -> Result<Member<'db>, E::Error> {
    effects.checkpoint(MemberSourceWork::Begin).await?;
    // TODO: There are many things that are not yet implemented here:
    // - `typing.Final`
    // - Proper diagnostics

    // NamedTuple fields are modeled via synthesized descriptors on the class. Treating them
    // as instance attributes here causes inherited fields to leak through after a subclass
    // shadows the name with a normal class attribute.
    effects.checkpoint(MemberSourceWork::CodeGenerator).await?;
    if let Some(CodeGeneratorKind::NamedTuple) = effects.code_generator(class).await?
        && {
            effects
                .checkpoint(MemberSourceWork::NamedTupleField)
                .await?;
            effects.has_own_named_tuple_field(class, name).await?
        }
    {
        effects.checkpoint(MemberSourceWork::Publish).await?;
        return Ok(Member::unbound());
    }

    effects.checkpoint(MemberSourceWork::ClassHeader).await?;
    let body_scope = effects.body_scope(class).await?;
    effects.checkpoint(MemberSourceWork::PlaceTable).await?;
    let table = effects.place_table(body_scope).await?;

    effects.checkpoint(MemberSourceWork::Symbol).await?;
    let member = if let Some(symbol_id) = effects.symbol_id(table, name).await? {
        effects.checkpoint(MemberSourceWork::UseDef).await?;
        let use_def = effects.use_def_map(body_scope).await?;

        let declarations = use_def.end_of_scope_symbol_declarations(symbol_id);
        effects.checkpoint(MemberSourceWork::Declarations).await?;
        let result = effects.declaration_place(env, declarations).await?;
        let imported = use_def.end_of_scope_imported_final_candidates(symbol_id.into());
        effects.checkpoint(MemberSourceWork::ImportedFinal).await?;
        let result = effects.imported_final(env, result, imported).await?;
        let declared_and_qualifiers = result.ignore_conflicting_declarations();
        effects.checkpoint(MemberSourceWork::Classify).await?;

        match declared_and_qualifiers {
            PlaceAndQualifiers {
                place:
                    mut declared @ Place::Defined(DefinedPlace {
                        ty: declared_ty,
                        definedness: declaredness,
                        provenance: declared_provenance,
                        ..
                    }),
                qualifiers,
            } => {
                // For the purpose of finding instance attributes, ignore `ClassVar`
                // declarations:
                if qualifiers.contains(TypeQualifiers::CLASS_VAR) {
                    declared = Place::Undefined;
                }

                if qualifiers.contains(TypeQualifiers::INIT_VAR) {
                    // We ignore `InitVar` declarations on the class body, unless that attribute is overwritten
                    // by an implicit assignment in a method
                    if {
                        effects
                            .checkpoint(MemberSourceWork::ImplicitInference)
                            .await?;
                        effects.implicit_member(class, name).await?
                    }
                    .is_undefined()
                    {
                        effects.checkpoint(MemberSourceWork::Publish).await?;
                        return Ok(Member::unbound());
                    }
                }

                // `KW_ONLY` sentinels are markers, not real instance attributes.
                if {
                    effects.checkpoint(MemberSourceWork::KwOnly).await?;
                    effects.is_kw_only(declared_ty).await?
                } && {
                    effects.checkpoint(MemberSourceWork::CodeGenerator).await?;
                    effects.code_generator(class).await?
                }
                .is_some_and(CodeGeneratorKind::is_dataclass_like)
                {
                    effects.checkpoint(MemberSourceWork::Publish).await?;
                    return Ok(Member::unbound());
                }

                // The attribute is declared in the class body.

                let bindings = use_def.end_of_scope_symbol_bindings(symbol_id);
                effects.checkpoint(MemberSourceWork::Bindings).await?;
                let inferred = effects.binding_place(env, bindings).await?.place;
                // Stub assignments to slots describe instance storage, not runtime class
                // attributes.
                let has_binding = !(inferred.is_undefined() || {
                    effects.checkpoint(MemberSourceWork::Stub).await?;
                    effects.is_stub(class).await?
                } && {
                    effects.checkpoint(MemberSourceWork::InstanceSlot).await?;
                    effects.has_instance_slot(class, name).await?
                });

                if has_binding {
                    // The attribute is declared and bound in the class body.

                    let implicit = {
                        effects
                            .checkpoint(MemberSourceWork::ImplicitInference)
                            .await?;
                        effects.implicit_member(class, name).await?
                    };
                    if let Place::Defined(DefinedPlace {
                        ty: implicit_ty,
                        provenance: implicit_provenance,
                        ..
                    }) = implicit.inner.place
                    {
                        if declaredness == Definedness::AlwaysDefined {
                            // If a symbol is definitely declared, and we see
                            // attribute assignments in methods of the class,
                            // we trust the declared type.
                            Member {
                                inner: declared.with_qualifiers(qualifiers),
                            }
                        } else {
                            Member {
                                inner: Place::Defined(DefinedPlace {
                                    ty: {
                                        effects.checkpoint(MemberSourceWork::Union).await?;
                                        effects.union_two(env, declared_ty, implicit_ty).await?
                                    },
                                    origin: TypeOrigin::Declared,
                                    definedness: declaredness,
                                    public_type_policy: PublicTypePolicy::Raw,
                                    provenance: implicit_provenance.or(declared_provenance),
                                })
                                .with_qualifiers(qualifiers),
                            }
                        }
                    } else if {
                        effects.checkpoint(MemberSourceWork::DataclassField).await?;
                        effects.is_own_dataclass_instance_field(class, name).await?
                    } && {
                        effects.checkpoint(MemberSourceWork::Getter).await?;
                        effects.getter_member(env, declared_ty).await?
                    }
                    .place
                    .is_undefined()
                    {
                        // For dataclass-like classes, declared fields are assigned
                        // by the synthesized `__init__`, so they are instance
                        // attributes even without an explicit `self.x = ...`
                        // assignment in a method body.
                        //
                        // However, if the declared type is a descriptor (has
                        // `__get__`), we return unbound so that the descriptor
                        // protocol in `member_lookup_with_policy` can resolve
                        // the attribute type through `__get__`.
                        Member {
                            inner: declared.with_qualifiers(qualifiers),
                        }
                    } else {
                        // The symbol is declared and bound in the class body,
                        // but we did not find any attribute assignments in
                        // methods of the class. This means that the attribute
                        // has a class-level default value, but it would not be
                        // found in a `__dict__` lookup.

                        Member::unbound()
                    }
                } else {
                    // The attribute is declared but not bound in the class body.
                    // We take this as a sign that this is intended to be a pure
                    // instance attribute, and we trust the declared type, unless
                    // it is possibly-undeclared. In the latter case, we also
                    // union with the inferred type from attribute assignments.

                    if declaredness == Definedness::AlwaysDefined {
                        Member {
                            inner: declared.with_qualifiers(qualifiers),
                        }
                    } else {
                        if let Place::Defined(DefinedPlace {
                            ty: implicit_ty,
                            provenance: implicit_provenance,
                            ..
                        }) = {
                            effects
                                .checkpoint(MemberSourceWork::ImplicitInference)
                                .await?;
                            effects.implicit_member(class, name).await?
                        }
                        .inner
                        .place
                        {
                            Member {
                                inner: Place::Defined(DefinedPlace {
                                    ty: {
                                        effects.checkpoint(MemberSourceWork::Union).await?;
                                        effects.union_two(env, declared_ty, implicit_ty).await?
                                    },
                                    origin: TypeOrigin::Declared,
                                    definedness: declaredness,
                                    public_type_policy: PublicTypePolicy::Raw,
                                    provenance: implicit_provenance.or(declared_provenance),
                                })
                                .with_qualifiers(qualifiers),
                            }
                        } else {
                            Member {
                                inner: declared.with_qualifiers(qualifiers),
                            }
                        }
                    }
                }
            }

            PlaceAndQualifiers {
                place: Place::Undefined,
                qualifiers: _,
            } => {
                // The attribute is not *declared* in the class body. It could still be declared/bound
                // in a method.

                {
                    effects
                        .checkpoint(MemberSourceWork::ImplicitInference)
                        .await?;
                    effects.implicit_member(class, name).await?
                }
            }
        }
    } else {
        // This attribute is neither declared nor bound in the class body.
        // It could still be implicitly defined in a method.

        {
            effects
                .checkpoint(MemberSourceWork::ImplicitInference)
                .await?;
            effects.implicit_member(class, name).await?
        }
    };
    effects.checkpoint(MemberSourceWork::Publish).await?;
    Ok(member)
}

#[ty_mapping_probe_macros::dual_member_source]
pub(in crate::types) async fn runtime_binding_absent_with<'a, 'db, E: MemberSourceEffects<'db>>(
    env: &ProgramEnvironment<'db>,
    scope: ScopeId<'db>,
    name: &'a str,
    effects: &E,
) -> Result<bool, E::Error> {
    effects.checkpoint(MemberSourceWork::Begin).await?;
    effects.checkpoint(MemberSourceWork::PlaceTable).await?;
    let table = effects.place_table(scope).await?;
    effects.checkpoint(MemberSourceWork::Symbol).await?;
    let Some(symbol) = effects.symbol_id(table, name).await? else {
        effects.checkpoint(MemberSourceWork::Publish).await?;
        return Ok(false);
    };
    effects.checkpoint(MemberSourceWork::UseDef).await?;
    let use_def = effects.use_def_map(scope).await?;
    let bindings = use_def.end_of_scope_symbol_bindings(symbol);
    effects.checkpoint(MemberSourceWork::Bindings).await?;
    let binding = effects.binding_place(env, bindings).await?;
    effects.checkpoint(MemberSourceWork::Publish).await?;
    Ok(binding.place.is_undefined())
}

pub(in crate::types) struct InlineMemberSourceEffects<'db> {
    pub(super) db: &'db dyn Db,
}
impl<'db> InlineMemberSourceEffects<'db> {
    pub(in crate::types) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}
impl sealed::Sealed for InlineMemberSourceEffects<'_> {}
impl<'db> SynchronousMemberSourceEffects<'db> for InlineMemberSourceEffects<'db> {
    type Error = Infallible;
    fn checkpoint(&self, _work: MemberSourceWork) -> Result<(), Self::Error> {
        Ok(())
    }
    fn place_table(&self, scope: ScopeId<'db>) -> Result<&'db PlaceTable, Self::Error> {
        Ok(place_table(self.db, scope))
    }
    fn use_def_map(&self, scope: ScopeId<'db>) -> Result<&'db UseDefMap<'db>, Self::Error> {
        Ok(use_def_map(self.db, scope))
    }
    fn symbol_id(
        &self,
        table: &'db PlaceTable,
        name: &str,
    ) -> Result<Option<ScopedSymbolId>, Self::Error> {
        Ok(table.symbol_id(name))
    }
    fn binding_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        bindings: BindingWithConstraintsIterator<'map, 'db>,
    ) -> Result<PlaceWithDefinition<'db>, Self::Error> {
        Ok(place_from_bindings(self.db, env, bindings))
    }
}
impl<'db> SynchronousRawClassMemberEffects<'db> for InlineMemberSourceEffects<'db> {
    fn public_class_place(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(place_by_id(
            self.db,
            scope,
            symbol.into(),
            RequiresExplicitReExport::No,
            ConsideredDefinitions::EndOfScope,
        ))
    }
}
