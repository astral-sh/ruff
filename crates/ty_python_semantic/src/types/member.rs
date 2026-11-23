use crate::Db;
use crate::place::{
    ConsideredDefinitions, DefinedPlace, Place, PlaceAndQualifiers, RequiresExplicitReExport,
    TypeOrigin, place_by_id, place_from_bindings, place_from_declarations,
};
use crate::types::{
    ClassBase, ProgramEnvironment, StaticClassLiteral, Type, class::MroLookup,
    infer::nearest_enclosing_class,
};
use ty_python_core::{
    UseDefMap, definition::Definition, frozen::FrozenSet, place_table, scope::ScopeId,
    semantic_index, symbol::ScopedSymbolId, use_def_map,
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

/// A class-body annotation together with the class that declares it.
///
/// The declaring class is retained even when several declarations contribute to the member's
/// type, or an unannotated default inherits the annotation through an intermediate base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ClassBodyDeclaration<'db> {
    pub(super) member: PlaceAndQualifiers<'db>,
    pub(super) class: StaticClassLiteral<'db>,
}

/// Infer the public type of a class member/symbol (its type as seen from outside its scope) in the given
/// `scope`.
pub(super) fn class_member<'db>(db: &'db dyn Db, scope: ScopeId<'db>, name: &str) -> Member<'db> {
    class_member_with_symbol(db, scope, place_table(db, scope).symbol_id(name))
}

pub(super) fn class_member_with_symbol<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol: Option<ScopedSymbolId>,
) -> Member<'db> {
    symbol.map_or_default(|symbol_id| {
        let mut place_and_quals = place_by_id(
            db,
            scope,
            symbol_id.into(),
            RequiresExplicitReExport::No,
            ConsideredDefinitions::EndOfScope,
        );

        if let Place::Defined(ref mut place) = place_and_quals.place
            && place.origin == TypeOrigin::Inferred
            && let Some(inherited) =
                inherited_class_body_declaration(db, use_def_map(db, scope), symbol_id)
        {
            if let Place::Defined(declared) = inherited.member.place {
                // The annotation determines the public type, but the value is still supplied
                // by this class. Consumers such as Pydantic inspect that value's definition.
                *place = DefinedPlace {
                    ty: declared.ty,
                    origin: declared.origin,
                    public_type_policy: declared.public_type_policy,
                    ..*place
                };
            }
            place_and_quals.qualifiers = inherited.member.qualifiers;
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
}

/// Undeclared bindings whose names occur in a base class body, in symbol ID order.
/// Returns `None` when the bases aren't fully known and every name needs a lookup.
#[salsa::tracked(returns(ref), cycle_initial=|_, _, _, _| None, heap_size=ruff_memory_usage::heap_size)]
fn inherited_declaration_candidates<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
) -> Option<FrozenSet<ScopedSymbolId>> {
    let index = semantic_index(db, scope.program_file(db));
    let class = nearest_enclosing_class(db, index, scope)?;
    let mut bases = Vec::new();
    for base in class.identity_specialization(db).iter_mro(db).skip(1) {
        let base = match base {
            ClassBase::Dynamic(_) | ClassBase::Divergent(_) => return None,
            ClassBase::Any | ClassBase::Generic | ClassBase::Protocol | ClassBase::TypedDict(_) => {
                continue;
            }
            ClassBase::Class(base) => base,
        };
        let (base, _) = base.static_class_literal(db)?;
        bases.push(place_table(db, base.body_scope(db)));
    }

    let table = index.place_table(scope.file_scope_id(db));
    // Looking up a single member of a large enum should not require scanning
    // all its members if the base classes have only a few names.
    if bases.iter().map(|base| base.symbols().len()).sum::<usize>() < table.symbols().len() {
        let mut candidates: Vec<_> = bases
            .iter()
            .flat_map(|base| base.symbols())
            .filter_map(|symbol| table.symbol_id(symbol.name()))
            .filter(|id| {
                let symbol = table.symbol(*id);
                symbol.is_bound() && !symbol.is_declared()
            })
            .collect();
        candidates.sort_unstable();
        candidates.dedup();
        return Some(FrozenSet::from_sorted_unique(candidates));
    }

    let candidates = table
        .symbols()
        .enumerate()
        .filter(|(_, symbol)| symbol.is_bound() && !symbol.is_declared())
        .filter(|(_, symbol)| {
            bases
                .iter()
                .any(|base| base.symbol_id(symbol.name()).is_some())
        })
        .map(|(id, _)| ScopedSymbolId::from_usize(id));
    Some(FrozenSet::from_sorted_unique(candidates))
}

/// Returns the inherited class-body annotation governing an unannotated class attribute.
///
/// A subclass assignment such as `items = []` retains an inherited `items: list[int]`
/// declaration. Both initializer inference and public member lookup use that declaration,
/// while an explicit annotation or a new method definition supplies its own public type.
pub(super) fn inherited_class_body_declaration<'db>(
    db: &'db dyn Db,
    use_def: &UseDefMap<'db>,
    symbol: ScopedSymbolId,
) -> Option<ClassBodyDeclaration<'db>> {
    // All assignments and member lookups share one query for this symbol. Use its
    // first binding as the key to avoid interning a (scope, symbol) pair. Include
    // bindings that are later deleted, since their initializers still need context.
    let binding = use_def
        .reachable_symbol_bindings(symbol)
        .find_map(|binding| binding.binding.definition())?;
    inherited_declaration_by_binding(db, binding).copied()
}

/// Most class attributes are new, so keep negative results small by boxing the declaration.
#[salsa::tracked(returns(as_deref), cycle_initial=|_, _, _, _| None, heap_size=ruff_memory_usage::heap_size)]
fn inherited_declaration_by_binding<'db>(
    db: &'db dyn Db,
    binding: Definition<'db>,
) -> Option<Box<ClassBodyDeclaration<'db>>> {
    let scope = binding.scope(db);
    let symbol = binding.place(db).expect_symbol();
    let index = semantic_index(db, scope.program_file(db));
    let scope_id = scope.file_scope_id(db);
    index.scope(scope_id).node().as_class()?;
    let table = index.place_table(scope_id);

    // Resolving the bases can recursively look up this attribute. Keep the filter
    // inside the per-name query so a recursive lookup sees its provisional result.
    if !table.symbol(symbol).is_declared()
        && inherited_declaration_candidates(db, scope)
            .as_ref()
            .is_some_and(|candidates| !candidates.contains(&symbol))
    {
        return None;
    }
    let use_def = use_def_map(db, scope);
    let env = ProgramEnvironment::from_scope(scope);
    if !place_from_declarations(db, &env, use_def.end_of_scope_symbol_declarations(symbol))
        .ignore_conflicting_declarations()
        .is_undefined()
    {
        return None;
    }

    let class = nearest_enclosing_class(db, index, scope)?;
    class_body_declaration_from_bases(db, &env, class, table.symbol(symbol).name()).map(Box::new)
}

pub(super) fn class_body_declaration_from_bases<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: StaticClassLiteral<'db>,
    name: &str,
) -> Option<ClassBodyDeclaration<'db>> {
    let mut inherited = MroLookup::new(
        db,
        env,
        class.identity_specialization(db).iter_mro(db).skip(1),
    )
    .class_body_declaration(name)?;
    // Hash definitions are checked against the hash protocol, not inherited annotations.
    // Retain qualifiers such as `ClassVar`, which still constrain instance assignments.
    if name == "__hash__" {
        inherited.member.place = Place::Undefined;
    }
    Some(inherited)
}
