use crate::Db;
use crate::place::{
    ConsideredDefinitions, DefinedPlace, Place, PlaceAndQualifiers, RequiresExplicitReExport,
    TypeOrigin, place_by_id, place_from_bindings, place_from_declarations,
};
use crate::types::{
    ClassBase, ProgramEnvironment, Type, TypeQualifiers, infer::nearest_enclosing_class,
};
use ty_python_core::{
    definition::DefinitionKind, place_table, scope::ScopeId, semantic_index,
    symbol::ScopedSymbolId, use_def_map,
};

/// The return type of certain member-lookup operations. Contains information
/// about the type, type qualifiers, boundness/declaredness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize, Default, salsa::SalsaValue)]
pub(super) struct Member<'db> {
    /// Type, qualifiers, and boundness information of this member
    pub(super) inner: PlaceAndQualifiers<'db>,
}

impl<'db> Member<'db> {
    pub(super) fn unbound() -> Self {
        Self {
            inner: PlaceAndQualifiers::unbound(),
        }
    }

    pub(super) fn definitely_declared(ty: Type<'db>) -> Self {
        Self {
            inner: Place::declared(ty).into(),
        }
    }

    /// Returns the type qualifiers of this member.
    pub(super) fn qualifiers(&self) -> crate::types::TypeQualifiers {
        self.inner.qualifiers
    }

    /// Returns `true` if the inner place is undefined (i.e. there is no such member).
    pub(super) fn is_undefined(&self) -> bool {
        self.inner.place.is_undefined()
    }

    /// Returns the inner type, unless it is definitely undefined.
    pub(super) fn ignore_possibly_undefined(&self) -> Option<Type<'db>> {
        self.inner.place.ignore_possibly_undefined()
    }

    /// Map a type transformation function over the type of this member.
    #[must_use]
    pub(super) fn map_type(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Self {
        Self {
            inner: self.inner.map_type(f),
        }
    }
}

/// Infer the public type of a class member/symbol (its type as seen from outside its scope) in the given
/// `scope`.
pub(super) fn class_member<'db>(db: &'db dyn Db, scope: ScopeId<'db>, name: &str) -> Member<'db> {
    place_table(db, scope)
        .symbol_id(name)
        .map(|symbol_id| {
            let mut place_and_quals = place_by_id(
                db,
                scope,
                symbol_id.into(),
                RequiresExplicitReExport::No,
                ConsideredDefinitions::EndOfScope,
            );

            if let Place::Defined(ref mut place) = place_and_quals.place
                && place.origin == TypeOrigin::Inferred
                && let Some(inherited) = inherited_class_attribute_declaration(db, scope, symbol_id)
                && let Place::Defined(declared) = inherited.place
            {
                // The annotation determines the public type, but the value is still supplied
                // by this class. Consumers such as Pydantic inspect that value's definition.
                *place = DefinedPlace {
                    ty: declared.ty,
                    origin: declared.origin,
                    public_type_policy: declared.public_type_policy,
                    ..*place
                };
                place_and_quals.qualifiers = inherited.qualifiers;
            }

            if !place_and_quals.is_undefined() && !place_and_quals.is_init_var() {
                // Trust the declared type if we see a class-level declaration
                return Member {
                    inner: place_and_quals,
                };
            }

            if let PlaceAndQualifiers {
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
                let use_def = use_def_map(db, scope);
                let bindings = use_def.end_of_scope_symbol_bindings(symbol_id);
                let env = ProgramEnvironment::from_scope(scope);
                let inferred = place_from_bindings(db, &env, bindings).place;

                // TODO: we should not need to calculate inferred type second time. This is a temporary
                // solution until the notion of Boundness and Declaredness is split. See #16036, #16264
                Member {
                    inner: match inferred {
                        Place::Undefined => Place::Undefined.with_qualifiers(qualifiers),
                        Place::Defined(place) => Place::Defined(DefinedPlace {
                            ty,
                            provenance: place.provenance.or(declared_provenance),
                            ..place
                        })
                        .with_qualifiers(qualifiers),
                    },
                }
            } else {
                Member::unbound()
            }
        })
        .unwrap_or_default()
}

/// Returns the inherited annotation governing an unannotated class attribute.
///
/// A subclass assignment such as `items = []` retains an inherited `items: list[int]`
/// declaration. Both initializer inference and public member lookup use that declaration,
/// while an explicit annotation or a new method definition supplies its own public type.
#[salsa::tracked(returns(copy), cycle_initial=|_, _, _, _| None, heap_size=ruff_memory_usage::heap_size)]
pub(super) fn inherited_class_attribute_declaration<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol: ScopedSymbolId,
) -> Option<PlaceAndQualifiers<'db>> {
    scope.node(db).as_class()?;
    let table = place_table(db, scope);
    let name = table.symbol(symbol).name();
    let use_def = use_def_map(db, scope);
    let env = ProgramEnvironment::from_scope(scope);
    if !place_from_declarations(db, &env, use_def.end_of_scope_symbol_declarations(symbol))
        .ignore_conflicting_declarations()
        .is_undefined()
    {
        return None;
    }

    let class = nearest_enclosing_class(db, semantic_index(db, scope.program_file(db)), scope)?;
    let specialization = class
        .generic_context(db)
        .map(|context| context.identity_specialization(db));
    for base in class.iter_mro(db, specialization).skip(1) {
        let base = match base {
            ClassBase::Generic | ClassBase::Protocol => continue,
            ClassBase::Class(base) => base,
            _ => return None,
        };
        let (base, specialization) = base.static_class_literal(db)?;
        let scope = base.body_scope(db);
        let Some(symbol) = place_table(db, scope).symbol_id(name) else {
            continue;
        };
        if place_by_id(
            db,
            scope,
            symbol.into(),
            RequiresExplicitReExport::No,
            ConsideredDefinitions::EndOfScope,
        )
        .is_undefined()
        {
            continue;
        }
        let declarations = use_def_map(db, scope).end_of_scope_symbol_declarations(symbol);
        let declared = place_from_declarations(db, &env, declarations.clone())
            .ignore_conflicting_declarations();
        let declaration = if declared.is_undefined() {
            inherited_class_attribute_declaration(db, scope, symbol)
        } else {
            // Methods, imports, and nested classes declare their own types, but do not provide
            // annotations for subclass assignments. Inspect declarations directly: public member
            // provenance may instead describe a binding, or combine several definitions.
            (!declared.qualifiers.contains(TypeQualifiers::FINAL)
                && declarations
                    .filter_map(|declaration| declaration.declaration.definition())
                    .all(|definition| {
                        matches!(definition.kind(db), DefinitionKind::AnnotatedAssignment(_))
                    }))
            .then_some(declared)
        };
        return declaration.map(|declaration| {
            declaration.map_type(|ty| ty.apply_optional_specialization(db, specialization))
        });
    }
    None
}
