use ruff_db::files::File;
use ruff_python_ast::PythonVersion;
use ty_module_resolver::{
    KnownModule, Module, ModuleName, file_to_module, resolve_module_confident,
};

use crate::dunder_all::dunder_all_names;
use crate::semantic_index::definition::{Definition, DefinitionState};
use crate::semantic_index::place::{PlaceExprRef, ScopedPlaceId};
use crate::semantic_index::scope::ScopeId;
use crate::semantic_index::{
    BindingWithConstraints, BindingWithConstraintsIterator, DeclarationsIterator, place_table,
};
use crate::semantic_index::{DeclarationWithConstraint, global_scope, use_def_map};
use crate::types::{
    ApplyTypeMappingVisitor, DynamicType, KnownClass, MaterializationKind, MemberLookupPolicy,
    Truthiness, Type, TypeAndQualifiers, TypeQualifiers, UnionBuilder, UnionType, binding_type,
    declaration_type, todo_type,
};
use crate::{Db, FxOrderSet, Program};

pub(crate) use implicit_globals::{
    module_type_implicit_global_declaration, module_type_implicit_global_symbol,
};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, get_size2::GetSize)]
pub(crate) enum Definedness {
    AlwaysDefined,
    PossiblyUndefined,
}

impl Definedness {
    pub(crate) const fn max(self, other: Self) -> Self {
        match (self, other) {
            (Definedness::AlwaysDefined, _) | (_, Definedness::AlwaysDefined) => {
                Definedness::AlwaysDefined
            }
            (Definedness::PossiblyUndefined, Definedness::PossiblyUndefined) => {
                Definedness::PossiblyUndefined
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, get_size2::GetSize)]
pub(crate) enum TypeOrigin {
    Declared,
    Inferred,
}

impl TypeOrigin {
    pub(crate) const fn is_declared(self) -> bool {
        matches!(self, TypeOrigin::Declared)
    }

    pub(crate) const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (TypeOrigin::Declared, TypeOrigin::Declared) => TypeOrigin::Declared,
            _ => TypeOrigin::Inferred,
        }
    }
}

/// Whether a place's type should be widened with `Unknown` when accessed publicly.
///
/// For undeclared public symbols (e.g., class attributes without type annotations),
/// the gradual typing guarantee requires that we consider them as potentially
/// modified externally, so their type is widened to a union with `Unknown`.
///
/// This enum tracks whether such widening should be applied, allowing callers
/// to access either the raw inferred type or the widened public type.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default, get_size2::GetSize)]
pub(crate) enum Widening {
    /// The type should not be widened with `Unknown`.
    #[default]
    None,
    /// The type should be widened with `Unknown` when accessed publicly.
    WithUnknown,
}

impl Widening {
    /// Apply widening to the type if this is `WithUnknown`.
    pub(crate) fn apply_if_needed<'db>(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        match self {
            Self::None => ty,
            Self::WithUnknown => UnionType::from_elements(db, [Type::unknown(), ty]),
        }
    }
}

/// The result of a place lookup, which can either be a (possibly undefined) type
/// or a completely undefined place.
///
/// If a place has both a binding and a declaration, the result of the binding is used.
///
/// Consider this example:
/// ```py
/// bound = 1
/// declared: int
///
/// if flag:
///     possibly_unbound = 2
///     possibly_undeclared: int
///
/// if flag:
///     bound_or_declared = 1
/// else:
///     bound_or_declared: int
/// ```
///
/// If we look up places in this scope, we would get the following results:
/// ```rs
/// bound:               Place::Defined(Literal[1], TypeOrigin::Inferred, Definedness::AlwaysDefined, _),
/// declared:            Place::Defined(int, TypeOrigin::Declared, Definedness::AlwaysDefined, _),
/// possibly_unbound:    Place::Defined(Literal[2], TypeOrigin::Inferred, Definedness::PossiblyUndefined, _),
/// possibly_undeclared: Place::Defined(int, TypeOrigin::Declared, Definedness::PossiblyUndefined, _),
/// bound_or_declared:   Place::Defined(Literal[1], TypeOrigin::Inferred, Definedness::PossiblyUndefined, _),
/// non_existent:        Place::Undefined,
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) enum Place<'db> {
    Defined(Type<'db>, TypeOrigin, Definedness, Widening),
    Undefined,
}

impl<'db> Place<'db> {
    /// Constructor that creates a [`Place`] with type origin [`TypeOrigin::Inferred`] and definedness [`Definedness::AlwaysDefined`].
    pub(crate) fn bound(ty: impl Into<Type<'db>>) -> Self {
        Place::Defined(
            ty.into(),
            TypeOrigin::Inferred,
            Definedness::AlwaysDefined,
            Widening::None,
        )
    }

    /// Constructor that creates a [`Place`] with type origin [`TypeOrigin::Declared`] and definedness [`Definedness::AlwaysDefined`].
    pub(crate) fn declared(ty: impl Into<Type<'db>>) -> Self {
        Place::Defined(
            ty.into(),
            TypeOrigin::Declared,
            Definedness::AlwaysDefined,
            Widening::None,
        )
    }

    /// Constructor that creates a [`Place`] with a [`crate::types::TodoType`] type
    /// and definedness [`Definedness::AlwaysDefined`].
    #[allow(unused_variables)] // Only unused in release builds
    pub(crate) fn todo(message: &'static str) -> Self {
        Place::Defined(
            todo_type!(message),
            TypeOrigin::Inferred,
            Definedness::AlwaysDefined,
            Widening::None,
        )
    }

    pub(crate) fn is_undefined(&self) -> bool {
        matches!(self, Place::Undefined)
    }

    /// Returns the type of the place, ignoring possible undefinedness.
    ///
    /// If the place is *definitely* undefined, this function will return `None`. Otherwise,
    /// if there is at least one control-flow path where the place is defined, return the type.
    pub(crate) fn ignore_possibly_undefined(&self) -> Option<Type<'db>> {
        match self {
            Place::Defined(ty, _, _, _) => Some(*ty),
            Place::Undefined => None,
        }
    }

    /// Returns the type of the place without widening applied.
    ///
    /// The stored type is always the unwidened type. Widening (union with `Unknown`)
    /// is applied lazily when converting to `LookupResult`.
    pub(crate) fn unwidened_type(&self) -> Option<Type<'db>> {
        match self {
            Place::Defined(ty, _, _, _) => Some(*ty),
            Place::Undefined => None,
        }
    }

    #[cfg(test)]
    #[track_caller]
    pub(crate) fn expect_type(self) -> Type<'db> {
        self.ignore_possibly_undefined()
            .expect("Expected a (possibly undefined) type, not an undefined place")
    }

    #[must_use]
    pub(crate) fn map_type(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Place<'db> {
        match self {
            Place::Defined(ty, origin, definedness, widening) => {
                Place::Defined(f(ty), origin, definedness, widening)
            }
            Place::Undefined => Place::Undefined,
        }
    }

    /// Set the widening mode for this place.
    #[must_use]
    pub(crate) fn with_widening(self, widening: Widening) -> Place<'db> {
        match self {
            Place::Defined(ty, origin, definedness, _) => {
                Place::Defined(ty, origin, definedness, widening)
            }
            Place::Undefined => Place::Undefined,
        }
    }

    #[must_use]
    pub(crate) fn with_qualifiers(self, qualifiers: TypeQualifiers) -> PlaceAndQualifiers<'db> {
        PlaceAndQualifiers {
            place: self,
            qualifiers,
        }
    }

    /// Try to call `__get__(None, owner)` on the type of this place (not on the meta type).
    /// If it succeeds, return the `__get__` return type. Otherwise, returns the original place.
    /// This is used to resolve (potential) descriptor attributes.
    pub(crate) fn try_call_dunder_get(self, db: &'db dyn Db, owner: Type<'db>) -> Place<'db> {
        match self {
            Place::Defined(Type::Union(union), origin, definedness, widening) => union
                .map_with_boundness(db, |elem| {
                    Place::Defined(*elem, origin, definedness, widening)
                        .try_call_dunder_get(db, owner)
                }),

            Place::Defined(Type::Intersection(intersection), origin, definedness, widening) => {
                intersection.map_with_boundness(db, |elem| {
                    Place::Defined(*elem, origin, definedness, widening)
                        .try_call_dunder_get(db, owner)
                })
            }

            Place::Defined(self_ty, origin, definedness, widening) => {
                if let Some((dunder_get_return_ty, _)) =
                    self_ty.try_call_dunder_get(db, Type::none(db), owner)
                {
                    Place::Defined(dunder_get_return_ty, origin, definedness, widening)
                } else {
                    self
                }
            }

            Place::Undefined => Place::Undefined,
        }
    }

    pub(crate) const fn is_definitely_bound(&self) -> bool {
        matches!(self, Place::Defined(_, _, Definedness::AlwaysDefined, _))
    }
}

impl<'db> From<LookupResult<'db>> for PlaceAndQualifiers<'db> {
    fn from(value: LookupResult<'db>) -> Self {
        match value {
            Ok(type_and_qualifiers) => Place::bound(type_and_qualifiers.inner_type())
                .with_qualifiers(type_and_qualifiers.qualifiers()),
            Err(LookupError::Undefined(qualifiers)) => Place::Undefined.with_qualifiers(qualifiers),
            Err(LookupError::PossiblyUndefined(type_and_qualifiers)) => Place::Defined(
                type_and_qualifiers.inner_type(),
                TypeOrigin::Inferred,
                Definedness::PossiblyUndefined,
                Widening::None,
            )
            .with_qualifiers(type_and_qualifiers.qualifiers()),
        }
    }
}

/// Possible ways in which a place lookup can (possibly or definitely) fail.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum LookupError<'db> {
    Undefined(TypeQualifiers),
    PossiblyUndefined(TypeAndQualifiers<'db>),
}

impl<'db> LookupError<'db> {
    /// Fallback (wholly or partially) to `fallback` to create a new [`LookupResult`].
    pub(crate) fn or_fall_back_to(
        self,
        db: &'db dyn Db,
        fallback: PlaceAndQualifiers<'db>,
    ) -> LookupResult<'db> {
        let fallback = fallback.into_lookup_result(db);
        match (&self, &fallback) {
            (LookupError::Undefined(_), _) => fallback,
            (LookupError::PossiblyUndefined { .. }, Err(LookupError::Undefined(_))) => Err(self),
            (LookupError::PossiblyUndefined(ty), Ok(ty2)) => Ok(TypeAndQualifiers::new(
                UnionType::from_elements(db, [ty.inner_type(), ty2.inner_type()]),
                ty.origin().merge(ty2.origin()),
                ty.qualifiers().union(ty2.qualifiers()),
            )),
            (LookupError::PossiblyUndefined(ty), Err(LookupError::PossiblyUndefined(ty2))) => {
                Err(LookupError::PossiblyUndefined(TypeAndQualifiers::new(
                    UnionType::from_elements(db, [ty.inner_type(), ty2.inner_type()]),
                    ty.origin().merge(ty2.origin()),
                    ty.qualifiers().union(ty2.qualifiers()),
                )))
            }
        }
    }
}

/// A [`Result`] type in which the `Ok` variant represents a definitely bound place
/// and the `Err` variant represents a place that is either definitely or possibly unbound.
///
/// Note that this type is exactly isomorphic to [`Place`].
/// In the future, we could possibly consider removing `Place` and using this type everywhere instead.
pub(crate) type LookupResult<'db> = Result<TypeAndQualifiers<'db>, LookupError<'db>>;

/// Infer the public type of a symbol (its type as seen from outside its scope) in the given
/// `scope`.
#[allow(unused)]
pub(crate) fn symbol<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
    considered_definitions: ConsideredDefinitions,
) -> PlaceAndQualifiers<'db> {
    symbol_impl(
        db,
        scope,
        name,
        RequiresExplicitReExport::No,
        considered_definitions,
    )
}

/// Infer the public type of a place (its type as seen from outside its scope) in the given
/// `scope`.
pub(crate) fn place<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    member: PlaceExprRef,
    considered_definitions: ConsideredDefinitions,
) -> PlaceAndQualifiers<'db> {
    place_impl(
        db,
        scope,
        member,
        RequiresExplicitReExport::No,
        considered_definitions,
    )
}

/// Infers the public type of an explicit module-global symbol as seen from within the same file.
///
/// Note that all global scopes also include various "implicit globals" such as `__name__`,
/// `__doc__` and `__file__`. This function **does not** consider those symbols; it will return
/// `Place::Undefined` for them. Use the (currently test-only) `global_symbol` query to also include
/// those additional symbols.
///
/// Use [`imported_symbol`] to perform the lookup as seen from outside the file (e.g. via imports).
pub(crate) fn explicit_global_symbol<'db>(
    db: &'db dyn Db,
    file: File,
    name: &str,
) -> PlaceAndQualifiers<'db> {
    symbol_impl(
        db,
        global_scope(db, file),
        name,
        RequiresExplicitReExport::No,
        ConsideredDefinitions::AllReachable,
    )
}

/// Infers the public type of an explicit module-global symbol as seen from within the same file.
///
/// Unlike [`explicit_global_symbol`], this function also considers various "implicit globals"
/// such as `__name__`, `__doc__` and `__file__`. These are looked up as attributes on `types.ModuleType`
/// rather than being looked up as symbols explicitly defined/declared in the global scope.
///
/// Use [`imported_symbol`] to perform the lookup as seen from outside the file (e.g. via imports).
#[allow(unused)]
pub(crate) fn global_symbol<'db>(
    db: &'db dyn Db,
    file: File,
    name: &str,
) -> PlaceAndQualifiers<'db> {
    explicit_global_symbol(db, file, name)
        .or_fall_back_to(db, || module_type_implicit_global_symbol(db, name))
}

/// Infers the public type of an imported symbol.
///
/// If `requires_explicit_reexport` is [`None`], it will be inferred from the file's source type.
/// For stub files, explicit re-export will be required, while for non-stub files, it will not.
pub(crate) fn imported_symbol<'db>(
    db: &'db dyn Db,
    file: File,
    name: &str,
    requires_explicit_reexport: Option<RequiresExplicitReExport>,
) -> PlaceAndQualifiers<'db> {
    let requires_explicit_reexport = requires_explicit_reexport.unwrap_or_else(|| {
        if file.is_stub(db) {
            RequiresExplicitReExport::Yes
        } else {
            RequiresExplicitReExport::No
        }
    });

    // If it's not found in the global scope, check if it's present as an instance on
    // `types.ModuleType` or `builtins.object`.
    //
    // We do a more limited version of this in `module_type_implicit_global_symbol`,
    // but there are two crucial differences here:
    // - If a member is looked up as an attribute, `__init__` is also available on the module, but
    //   it isn't available as a global from inside the module
    // - If a member is looked up as an attribute, members on `builtins.object` are also available
    //   (because `types.ModuleType` inherits from `object`); these attributes are also not
    //   available as globals from inside the module.
    //
    // The same way as in `module_type_implicit_global_symbol`, however, we need to be careful to
    // ignore `__getattr__`. Typeshed has a fake `__getattr__` on `types.ModuleType` to help out with
    // dynamic imports; we shouldn't use it for `ModuleLiteral` types where we know exactly which
    // module we're dealing with.
    symbol_impl(
        db,
        global_scope(db, file),
        name,
        requires_explicit_reexport,
        ConsideredDefinitions::EndOfScope,
    )
    .or_fall_back_to(db, || {
        if name == "__getattr__" {
            Place::Undefined.into()
        } else if name == "__builtins__" {
            Place::bound(Type::any()).into()
        } else {
            KnownClass::ModuleType
                .to_instance(db)
                .member_lookup_with_policy(db, name.into(), MemberLookupPolicy::NO_GETATTR_LOOKUP)
        }
    })
}

/// Lookup the type of `symbol` in the builtins namespace.
///
/// Returns `Place::Undefined` if the `builtins` module isn't available for some reason.
///
/// Note that this function is only intended for use in the context of the builtins *namespace*
/// and should not be used when a symbol is being explicitly imported from the `builtins` module
/// (e.g. `from builtins import int`).
pub(crate) fn builtins_symbol<'db>(db: &'db dyn Db, symbol: &str) -> PlaceAndQualifiers<'db> {
    let resolver = |module: Module<'_>| {
        let file = module.file(db)?;
        let found_symbol = symbol_impl(
            db,
            global_scope(db, file),
            symbol,
            RequiresExplicitReExport::Yes,
            ConsideredDefinitions::EndOfScope,
        )
        .or_fall_back_to(db, || {
            // We're looking up in the builtins namespace and not the module, so we should
            // do the normal lookup in `types.ModuleType` and not the special one as in
            // `imported_symbol`.
            module_type_implicit_global_symbol(db, symbol)
        });
        // If this symbol is not present in project-level builtins, search in the default ones.
        found_symbol
            .ignore_possibly_undefined()
            .map(|_| found_symbol)
    };
    resolve_module_confident(db, &ModuleName::new_static("__builtins__").unwrap())
        .and_then(&resolver)
        .or_else(|| resolve_module_confident(db, &KnownModule::Builtins.name()).and_then(resolver))
        .unwrap_or_default()
}

/// Lookup the type of `symbol` in a given known module.
///
/// Returns `Place::Undefined` if the given known module cannot be resolved for some reason.
pub(crate) fn known_module_symbol<'db>(
    db: &'db dyn Db,
    known_module: KnownModule,
    symbol: &str,
) -> PlaceAndQualifiers<'db> {
    resolve_module_confident(db, &known_module.name())
        .and_then(|module| {
            let file = module.file(db)?;
            Some(imported_symbol(db, file, symbol, None))
        })
        .unwrap_or_default()
}

/// Lookup the type of `symbol` in the `typing` module namespace.
///
/// Returns `Place::Undefined` if the `typing` module isn't available for some reason.
#[inline]
#[cfg(test)]
pub(crate) fn typing_symbol<'db>(db: &'db dyn Db, symbol: &str) -> PlaceAndQualifiers<'db> {
    known_module_symbol(db, KnownModule::Typing, symbol)
}

/// Lookup the type of `symbol` in the `typing_extensions` module namespace.
///
/// Returns `Place::Undefined` if the `typing_extensions` module isn't available for some reason.
#[inline]
pub(crate) fn typing_extensions_symbol<'db>(
    db: &'db dyn Db,
    symbol: &str,
) -> PlaceAndQualifiers<'db> {
    known_module_symbol(db, KnownModule::TypingExtensions, symbol)
}

/// Get the `builtins` module scope.
///
/// Can return `None` if a custom typeshed is used that is missing `builtins.pyi`.
pub(crate) fn builtins_module_scope(db: &dyn Db) -> Option<ScopeId<'_>> {
    core_module_scope(db, KnownModule::Builtins)
}

/// Get the scope of a core stdlib module.
///
/// Can return `None` if a custom typeshed is used that is missing the core module in question.
fn core_module_scope(db: &dyn Db, core_module: KnownModule) -> Option<ScopeId<'_>> {
    let module = resolve_module_confident(db, &core_module.name())?;
    Some(global_scope(db, module.file(db)?))
}

/// Infer the combined type from an iterator of bindings, and return it
/// together with boundness information in a [`Place`].
///
/// The type will be a union if there are multiple bindings with different types.
pub(super) fn place_from_bindings<'db>(
    db: &'db dyn Db,
    bindings_with_constraints: BindingWithConstraintsIterator<'_, 'db>,
) -> PlaceWithDefinition<'db> {
    place_from_bindings_impl(db, bindings_with_constraints, RequiresExplicitReExport::No)
}

/// Build a declared type from a [`DeclarationsIterator`].
///
/// If there is only one declaration, or all declarations declare the same type, returns
/// `Ok(..)`. If there are conflicting declarations, returns an `Err(..)` variant with
/// a union of the declared types as well as a list of all conflicting types.
///
/// This function also returns declaredness information (see [`Place`]) and a set of
/// [`TypeQualifiers`] that have been specified on the declaration(s).
pub(crate) fn place_from_declarations<'db>(
    db: &'db dyn Db,
    declarations: DeclarationsIterator<'_, 'db>,
) -> PlaceFromDeclarationsResult<'db> {
    place_from_declarations_impl(db, declarations, RequiresExplicitReExport::No)
}

type DeclaredTypeAndConflictingTypes<'db> = (
    TypeAndQualifiers<'db>,
    Option<Box<indexmap::set::Slice<Type<'db>>>>,
);

/// The result of looking up a declared type from declarations; see [`place_from_declarations`].
pub(crate) struct PlaceFromDeclarationsResult<'db> {
    place_and_quals: PlaceAndQualifiers<'db>,
    conflicting_types: Option<Box<indexmap::set::Slice<Type<'db>>>>,
    /// Contains the first reachable declaration for this place, if any.
    /// This field is used for backreferences in diagnostics.
    pub(crate) first_declaration: Option<Definition<'db>>,
}

impl<'db> PlaceFromDeclarationsResult<'db> {
    fn conflict(
        place_and_quals: PlaceAndQualifiers<'db>,
        conflicting_types: Box<indexmap::set::Slice<Type<'db>>>,
        first_declaration: Option<Definition<'db>>,
    ) -> Self {
        PlaceFromDeclarationsResult {
            place_and_quals,
            conflicting_types: Some(conflicting_types),
            first_declaration,
        }
    }

    pub(crate) fn ignore_conflicting_declarations(self) -> PlaceAndQualifiers<'db> {
        self.place_and_quals
    }

    pub(crate) fn into_place_and_conflicting_declarations(
        self,
    ) -> (
        PlaceAndQualifiers<'db>,
        Option<Box<indexmap::set::Slice<Type<'db>>>>,
    ) {
        (self.place_and_quals, self.conflicting_types)
    }
}

/// A type with declaredness information, and a set of type qualifiers.
///
/// This is used to represent the result of looking up the declared type. Consider this
/// example:
/// ```py
/// class C:
///     if flag:
///         variable: ClassVar[int]
/// ```
/// If we look up the declared type of `variable` in the scope of class `C`, we will get
/// the type `int`, a "declaredness" of [`Definedness::PossiblyUndefined`], and the information
/// that this comes with a [`CLASS_VAR`] type qualifier.
///
/// [`CLASS_VAR`]: crate::types::TypeQualifiers::CLASS_VAR
#[derive(Debug, Clone, Copy, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) struct PlaceAndQualifiers<'db> {
    pub(crate) place: Place<'db>,
    pub(crate) qualifiers: TypeQualifiers,
}

impl Default for PlaceAndQualifiers<'_> {
    fn default() -> Self {
        PlaceAndQualifiers {
            place: Place::Undefined,
            qualifiers: TypeQualifiers::empty(),
        }
    }
}

impl<'db> PlaceAndQualifiers<'db> {
    /// Constructor that creates a [`PlaceAndQualifiers`] instance with a [`TodoType`] type
    /// and no qualifiers.
    ///
    /// [`TodoType`]: crate::types::TodoType
    pub(crate) fn todo(message: &'static str) -> Self {
        Self {
            place: Place::todo(message),
            qualifiers: TypeQualifiers::empty(),
        }
    }

    pub(crate) fn unbound() -> Self {
        PlaceAndQualifiers {
            place: Place::Undefined,
            qualifiers: TypeQualifiers::empty(),
        }
    }

    pub(crate) fn is_undefined(&self) -> bool {
        self.place.is_undefined()
    }

    pub(crate) fn ignore_possibly_undefined(&self) -> Option<Type<'db>> {
        self.place.ignore_possibly_undefined()
    }

    /// Returns `true` if the place has a `ClassVar` type qualifier.
    pub(crate) fn is_class_var(&self) -> bool {
        self.qualifiers.contains(TypeQualifiers::CLASS_VAR)
    }

    /// Returns `true` if the place has a `InitVar` type qualifier.
    pub(crate) fn is_init_var(&self) -> bool {
        self.qualifiers.contains(TypeQualifiers::INIT_VAR)
    }

    /// Returns `true` if the place has a `Required` type qualifier.
    pub(crate) fn is_required(&self) -> bool {
        self.qualifiers.contains(TypeQualifiers::REQUIRED)
    }

    /// Returns `true` if the place has a `NotRequired` type qualifier.
    pub(crate) fn is_not_required(&self) -> bool {
        self.qualifiers.contains(TypeQualifiers::NOT_REQUIRED)
    }

    /// Returns `true` if the place has a `ReadOnly` type qualifier.
    pub(crate) fn is_read_only(&self) -> bool {
        self.qualifiers.contains(TypeQualifiers::READ_ONLY)
    }

    /// Returns `Some(…)` if the place is qualified with `typing.Final` without a specified type.
    pub(crate) fn is_bare_final(&self) -> Option<TypeQualifiers> {
        match self {
            PlaceAndQualifiers { place, qualifiers }
                if (qualifiers.contains(TypeQualifiers::FINAL)
                    && place
                        .ignore_possibly_undefined()
                        .is_some_and(|ty| ty.is_unknown())) =>
            {
                Some(*qualifiers)
            }
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn map_type(
        self,
        f: impl FnOnce(Type<'db>) -> Type<'db>,
    ) -> PlaceAndQualifiers<'db> {
        PlaceAndQualifiers {
            place: self.place.map_type(f),
            qualifiers: self.qualifiers,
        }
    }

    pub(crate) fn materialize(
        self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> PlaceAndQualifiers<'db> {
        self.map_type(|ty| ty.materialize(db, materialization_kind, visitor))
    }

    /// Transform place and qualifiers into a [`LookupResult`],
    /// a [`Result`] type in which the `Ok` variant represents a definitely defined place
    /// and the `Err` variant represents a place that is either definitely or possibly undefined.
    ///
    /// For places marked with `Widening::WithUnknown`, this applies the gradual typing guarantee
    /// by creating a union with `Unknown`.
    pub(crate) fn into_lookup_result(self, db: &'db dyn Db) -> LookupResult<'db> {
        match self {
            PlaceAndQualifiers {
                place: Place::Defined(ty, origin, Definedness::AlwaysDefined, widening),
                qualifiers,
            } => {
                let ty = widening.apply_if_needed(db, ty);
                Ok(TypeAndQualifiers::new(ty, origin, qualifiers))
            }
            PlaceAndQualifiers {
                place: Place::Defined(ty, origin, Definedness::PossiblyUndefined, widening),
                qualifiers,
            } => {
                let ty = widening.apply_if_needed(db, ty);
                Err(LookupError::PossiblyUndefined(TypeAndQualifiers::new(
                    ty, origin, qualifiers,
                )))
            }
            PlaceAndQualifiers {
                place: Place::Undefined,
                qualifiers,
            } => Err(LookupError::Undefined(qualifiers)),
        }
    }

    /// Safely unwrap the place and the qualifiers into a [`TypeAndQualifiers`].
    ///
    /// If the place is definitely unbound or possibly unbound, it will be transformed into a
    /// [`LookupError`] and `diagnostic_fn` will be applied to the error value before returning
    /// the result of `diagnostic_fn` (which will be a [`TypeAndQualifiers`]). This allows the caller
    /// to ensure that a diagnostic is emitted if the place is possibly or definitely unbound.
    pub(crate) fn unwrap_with_diagnostic(
        self,
        db: &'db dyn Db,
        diagnostic_fn: impl FnOnce(LookupError<'db>) -> TypeAndQualifiers<'db>,
    ) -> TypeAndQualifiers<'db> {
        self.into_lookup_result(db).unwrap_or_else(diagnostic_fn)
    }

    /// Fallback (partially or fully) to another place if `self` is partially or fully unbound.
    ///
    /// 1. If `self` is definitely bound, return `self` without evaluating `fallback_fn()`.
    /// 2. Else, evaluate `fallback_fn()`:
    ///    1. If `self` is definitely unbound, return the result of `fallback_fn()`.
    ///    2. Else, if `fallback` is definitely unbound, return `self`.
    ///    3. Else, if `self` is possibly unbound and `fallback` is definitely bound,
    ///       return `Place(<union of self-type and fallback-type>, Definedness::AlwaysDefined)`
    ///    4. Else, if `self` is possibly unbound and `fallback` is possibly unbound,
    ///       return `Place(<union of self-type and fallback-type>, Definedness::PossiblyUndefined)`
    #[must_use]
    pub(crate) fn or_fall_back_to(
        self,
        db: &'db dyn Db,
        fallback_fn: impl FnOnce() -> PlaceAndQualifiers<'db>,
    ) -> Self {
        self.into_lookup_result(db)
            .or_else(|lookup_error| lookup_error.or_fall_back_to(db, fallback_fn()))
            .into()
    }

    pub(crate) fn cycle_normalized(
        self,
        db: &'db dyn Db,
        previous_place: Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        let place = match (previous_place.place, self.place) {
            // In fixed-point iteration of type inference, the member type must be monotonically widened and not "oscillate".
            // Here, monotonicity is guaranteed by pre-unioning the type of the previous iteration into the current result.
            (
                Place::Defined(prev_ty, _, _, _),
                Place::Defined(ty, origin, definedness, widening),
            ) => Place::Defined(
                ty.cycle_normalized(db, prev_ty, cycle),
                origin,
                definedness,
                widening,
            ),
            // If a `Place` in the current cycle is `Defined` but `Undefined` in the previous cycle,
            // that means that its definedness depends on the truthiness of the previous cycle value.
            // In this case, the definedness of the current cycle `Place` is set to `PossiblyUndefined`.
            // Actually, this branch is unreachable. We evaluate the truthiness of non-definitely-bound places as Ambiguous (see #19579),
            // so convergence is guaranteed without resorting to this handling.
            // However, the handling described above may reduce the exactness of reachability analysis,
            // so it may be better to remove it. In that case, this branch is necessary.
            (Place::Undefined, Place::Defined(ty, origin, _definedness, widening)) => {
                Place::Defined(
                    ty.recursive_type_normalized(db, cycle),
                    origin,
                    Definedness::PossiblyUndefined,
                    widening,
                )
            }
            // If a `Place` that was `Defined(Divergent)` in the previous cycle is actually found to be unreachable in the current cycle,
            // it is set to `Undefined` (because the cycle initial value does not include meaningful reachability information).
            (Place::Defined(ty, origin, _definedness, widening), Place::Undefined) => {
                if cycle.head_ids().any(|id| ty == Type::divergent(id)) {
                    Place::Undefined
                } else {
                    Place::Defined(
                        ty.recursive_type_normalized(db, cycle),
                        origin,
                        Definedness::PossiblyUndefined,
                        widening,
                    )
                }
            }
            (Place::Undefined, Place::Undefined) => Place::Undefined,
        };
        PlaceAndQualifiers {
            place,
            qualifiers: self.qualifiers,
        }
    }
}

impl<'db> From<Place<'db>> for PlaceAndQualifiers<'db> {
    fn from(place: Place<'db>) -> Self {
        place.with_qualifiers(TypeQualifiers::empty())
    }
}

fn place_cycle_initial<'db>(
    _db: &'db dyn Db,
    id: salsa::Id,
    _scope: ScopeId<'db>,
    _place_id: ScopedPlaceId,
    _requires_explicit_reexport: RequiresExplicitReExport,
    _considered_definitions: ConsideredDefinitions,
) -> PlaceAndQualifiers<'db> {
    Place::bound(Type::divergent(id)).into()
}

#[allow(clippy::too_many_arguments)]
fn place_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous_place: &PlaceAndQualifiers<'db>,
    place: PlaceAndQualifiers<'db>,
    _scope: ScopeId<'db>,
    _place_id: ScopedPlaceId,
    _requires_explicit_reexport: RequiresExplicitReExport,
    _considered_definitions: ConsideredDefinitions,
) -> PlaceAndQualifiers<'db> {
    place.cycle_normalized(db, *previous_place, cycle)
}

#[salsa::tracked(cycle_fn=place_cycle_recover, cycle_initial=place_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn place_by_id<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    place_id: ScopedPlaceId,
    requires_explicit_reexport: RequiresExplicitReExport,
    considered_definitions: ConsideredDefinitions,
) -> PlaceAndQualifiers<'db> {
    let use_def = use_def_map(db, scope);

    // If the place is declared, the public type is based on declarations; otherwise, it's based
    // on inference from bindings.

    let declarations = match considered_definitions {
        ConsideredDefinitions::EndOfScope => use_def.end_of_scope_declarations(place_id),
        ConsideredDefinitions::AllReachable => use_def.reachable_declarations(place_id),
    };

    let declared = place_from_declarations_impl(db, declarations, requires_explicit_reexport)
        .ignore_conflicting_declarations();

    let all_considered_bindings = || match considered_definitions {
        ConsideredDefinitions::EndOfScope => use_def.end_of_scope_bindings(place_id),
        ConsideredDefinitions::AllReachable => use_def.reachable_bindings(place_id),
    };

    // If a symbol is undeclared, but qualified with `typing.Final`, we use the right-hand side
    // inferred type, without unioning with `Unknown`, because it cannot be modified.
    if let Some(qualifiers) = declared.is_bare_final() {
        let bindings = all_considered_bindings();
        return place_from_bindings_impl(db, bindings, requires_explicit_reexport)
            .place
            .with_qualifiers(qualifiers);
    }

    match declared {
        // Handle bare `ClassVar` annotations by falling back to the union of `Unknown` and the
        // inferred type.
        PlaceAndQualifiers {
            place: Place::Defined(Type::Dynamic(DynamicType::Unknown), origin, definedness, _),
            qualifiers,
        } if qualifiers.contains(TypeQualifiers::CLASS_VAR) => {
            let bindings = all_considered_bindings();
            match place_from_bindings_impl(db, bindings, requires_explicit_reexport).place {
                Place::Defined(inferred, origin, boundness, _) => Place::Defined(
                    UnionType::from_elements(db, [Type::unknown(), inferred]),
                    origin,
                    boundness,
                    Widening::None,
                )
                .with_qualifiers(qualifiers),
                Place::Undefined => {
                    Place::Defined(Type::unknown(), origin, definedness, Widening::None)
                        .with_qualifiers(qualifiers)
                }
            }
        }
        // Place is declared, trust the declared type
        place_and_quals @ PlaceAndQualifiers {
            place: Place::Defined(_, _, Definedness::AlwaysDefined, _),
            qualifiers: _,
        } => place_and_quals,
        // Place is possibly declared
        PlaceAndQualifiers {
            place: Place::Defined(declared_ty, origin, Definedness::PossiblyUndefined, _),
            qualifiers,
        } => {
            let bindings = all_considered_bindings();
            let boundness_analysis = bindings.boundness_analysis;
            let inferred = place_from_bindings_impl(db, bindings, requires_explicit_reexport);

            let place = match inferred.place {
                // Place is possibly undeclared and definitely unbound
                Place::Undefined => {
                    // TODO: We probably don't want to report `AlwaysDefined` here. This requires a bit of
                    // design work though as we might want a different behavior for stubs and for
                    // normal modules.
                    Place::Defined(
                        declared_ty,
                        origin,
                        Definedness::AlwaysDefined,
                        Widening::None,
                    )
                }
                // Place is possibly undeclared and (possibly) bound
                Place::Defined(inferred_ty, origin, boundness, _) => Place::Defined(
                    UnionType::from_elements(db, [inferred_ty, declared_ty]),
                    origin,
                    if boundness_analysis == BoundnessAnalysis::AssumeBound {
                        Definedness::AlwaysDefined
                    } else {
                        boundness
                    },
                    Widening::None,
                ),
            };

            PlaceAndQualifiers { place, qualifiers }
        }
        // Place is undeclared, infer the type from bindings
        PlaceAndQualifiers {
            place: Place::Undefined,
            qualifiers: _,
        } => {
            let bindings = all_considered_bindings();
            let boundness_analysis = bindings.boundness_analysis;
            let mut inferred =
                place_from_bindings_impl(db, bindings, requires_explicit_reexport).place;

            if boundness_analysis == BoundnessAnalysis::AssumeBound {
                if let Place::Defined(ty, origin, Definedness::PossiblyUndefined, widening) =
                    inferred
                {
                    inferred = Place::Defined(ty, origin, Definedness::AlwaysDefined, widening);
                }
            }

            // `__slots__` is a symbol with special behavior in Python's runtime. It can be
            // modified externally, but those changes do not take effect. We therefore issue
            // a diagnostic if we see it being modified externally. In type inference, we
            // can assign a "narrow" type to it even if it is not *declared*. This means, we
            // do not have to union with `Unknown`.
            //
            // `TYPE_CHECKING` is a special variable that should only be assigned `False`
            // at runtime, but is always considered `True` in type checking.
            // See mdtest/known_constants.md#user-defined-type_checking for details.
            let is_considered_non_modifiable = place_id.as_symbol().is_some_and(|symbol_id| {
                matches!(
                    place_table(db, scope).symbol(symbol_id).name().as_str(),
                    "__slots__" | "TYPE_CHECKING"
                )
            });

            // Module-level globals can be mutated externally. A `MY_CONSTANT = 1` global might
            // be changed to `"some string"` from code outside of the module that we're looking
            // at, and so from a gradual-guarantee perspective, it makes sense to infer a type
            // of `Literal[1] | Unknown` for global symbols. This allows the code that does the
            // mutation to type check correctly, and for code that uses the global, it accurately
            // reflects the lack of knowledge about the type.
            //
            // However, external modifications (or modifications through `global` statements) that
            // would require a wider type are relatively rare. From a practical perspective, we can
            // therefore achieve a better user experience by trusting the inferred type. Users who
            // need the external mutation to work can always annotate the global with the wider
            // type. And everyone else benefits from more precise type inference.
            let is_module_global = scope.node(db).scope_kind().is_module();

            // If the visibility of the scope is private (like for a function scope), we also do
            // not union with `Unknown`, because the symbol cannot be modified externally.
            let scope_has_private_visibility = scope.scope(db).visibility().is_private();

            // We generally trust undeclared places in stubs and do not union with `Unknown`.
            let in_stub_file = scope.file(db).is_stub(db);

            if is_considered_non_modifiable
                || is_module_global
                || scope_has_private_visibility
                || in_stub_file
            {
                inferred.into()
            } else {
                // Gradual typing guarantee: Mark undeclared public symbols for widening.
                // The actual union with `Unknown` is applied lazily when converting to
                // LookupResult via `into_lookup_result`.
                inferred.with_widening(Widening::WithUnknown).into()
            }
        }
    }

    // TODO (ticket: https://github.com/astral-sh/ruff/issues/14297) Our handling of boundness
    // currently only depends on bindings, and ignores declarations. This is inconsistent, since
    // we only look at bindings if the place may be undeclared. Consider the following example:
    // ```py
    // x: int
    //
    // if flag:
    //     y: int
    // else
    //     y = 3
    // ```
    // If we import from this module, we will currently report `x` as a definitely-bound place
    // (even though it has no bindings at all!) but report `y` as possibly-unbound (even though
    // every path has either a binding or a declaration for it.)
}

/// Implementation of [`symbol`].
fn symbol_impl<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
    requires_explicit_reexport: RequiresExplicitReExport,
    considered_definitions: ConsideredDefinitions,
) -> PlaceAndQualifiers<'db> {
    let _span = tracing::trace_span!("symbol", ?name).entered();

    if name == "platform"
        && file_to_module(db, scope.file(db))
            .is_some_and(|module| module.is_known(db, KnownModule::Sys))
    {
        match Program::get(db).python_platform(db) {
            crate::PythonPlatform::Identifier(platform) => {
                return Place::bound(Type::string_literal(db, platform.as_str())).into();
            }
            crate::PythonPlatform::All => {
                // Fall through to the looked up type
            }
        }
    }

    place_table(db, scope)
        .symbol_id(name)
        .map(|symbol| {
            place_by_id(
                db,
                scope,
                symbol.into(),
                requires_explicit_reexport,
                considered_definitions,
            )
        })
        .unwrap_or_default()
}

fn place_impl<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    place: PlaceExprRef,
    requires_explicit_reexport: RequiresExplicitReExport,
    considered_definitions: ConsideredDefinitions,
) -> PlaceAndQualifiers<'db> {
    let _span = tracing::trace_span!("place_impl", ?place).entered();

    place_table(db, scope)
        .place_id(place)
        .map(|place| {
            place_by_id(
                db,
                scope,
                place,
                requires_explicit_reexport,
                considered_definitions,
            )
        })
        .unwrap_or_default()
}

/// Implementation of [`place_from_bindings`].
///
/// ## Implementation Note
/// This function gets called cross-module. It, therefore, shouldn't
/// access any AST nodes from the file containing the declarations.
fn place_from_bindings_impl<'db>(
    db: &'db dyn Db,
    bindings_with_constraints: BindingWithConstraintsIterator<'_, 'db>,
    requires_explicit_reexport: RequiresExplicitReExport,
) -> PlaceWithDefinition<'db> {
    let predicates = bindings_with_constraints.predicates;
    let reachability_constraints = bindings_with_constraints.reachability_constraints;
    let boundness_analysis = bindings_with_constraints.boundness_analysis;
    let mut bindings_with_constraints = bindings_with_constraints.peekable();

    let is_non_exported = |binding: Definition<'db>| {
        requires_explicit_reexport.is_yes() && !is_reexported(db, binding)
    };

    let unbound_reachability_constraint = match bindings_with_constraints.peek() {
        Some(BindingWithConstraints {
            binding,
            reachability_constraint,
            narrowing_constraint: _,
        }) if binding.is_undefined_or(is_non_exported) => Some(*reachability_constraint),
        _ => None,
    };
    let mut deleted_reachability = Truthiness::AlwaysFalse;

    // Evaluate this lazily because we don't always need it (for example, if there are no visible
    // bindings at all, we don't need it), and it can cause us to evaluate reachability constraint
    // expressions, which is extra work and can lead to cycles.
    let unbound_visibility = || {
        unbound_reachability_constraint.map(|reachability_constraint| {
            reachability_constraints.evaluate(db, predicates, reachability_constraint)
        })
    };

    let mut first_definition = None;

    let mut types = bindings_with_constraints.filter_map(
        |BindingWithConstraints {
             binding,
             narrowing_constraint,
             reachability_constraint,
         }| {
            let binding = match binding {
                DefinitionState::Defined(binding) => binding,
                DefinitionState::Undefined => {
                    return None;
                }
                DefinitionState::Deleted => {
                    deleted_reachability = deleted_reachability.or(
                        reachability_constraints.evaluate(db, predicates, reachability_constraint)
                    );
                    return None;
                }
            };

            if is_non_exported(binding) {
                return None;
            }

            let static_reachability =
                reachability_constraints.evaluate(db, predicates, reachability_constraint);

            if static_reachability.is_always_false() {
                // If the static reachability evaluates to false, the binding is either not reachable
                // from the start of the scope, or there is no control flow path from that binding to
                // the use of the place that we are investigating. There are three interesting cases
                // to consider:
                //
                // ```py
                // def f1():
                //     if False:
                //         x = 1
                //     use(x)
                //
                // def f2():
                //     y = 1
                //     return
                //     use(y)
                //
                // def f3(flag: bool):
                //     if flag:
                //         z = 1
                //     else:
                //         z = 2
                //         return
                //     use(z)
                // ```
                //
                // In the first case, there is a single binding for `x`, but it is not reachable from
                // the start of the scope. However, the use of `x` is reachable (`unbound_reachability`
                // is not always-false). This means that `x` is unbound and we should return `None`.
                //
                // In the second case, the binding of `y` is reachable, but there is no control flow
                // path from the beginning of the scope, through that binding, to the use of `y` that
                // we are investigating. There is also no control flow path from the start of the
                // scope, through the implicit `y = <unbound>` binding, to the use of `y`. This means
                // that `unbound_reachability` is always false. Since there are no other bindings, no
                // control flow path can reach this use of `y`, implying that we are in unreachable
                // section of code. We return `Never` in order to silence the `unresolve-reference`
                // diagnostic that would otherwise be emitted at the use of `y`.
                //
                // In the third case, we have two bindings for `z`. The first one is visible (there
                // is a path of control flow from the start of the scope, through that binding, to
                // the use of `z`). So we consider the case that we now encounter the second binding
                // `z = 2`, which is not visible due to the early return. The `z = <unbound>` binding
                // is not live (shadowed by the other bindings), so `unbound_reachability` is `None`.
                // Here, we are *not* in an unreachable section of code. However, it is still okay to
                // return `Never` in this case, because we will union the types of all bindings, and
                // `Never` will be eliminated automatically.

                if unbound_visibility().is_none_or(Truthiness::is_always_false) {
                    return Some(Type::Never);
                }
                return None;
            }

            first_definition.get_or_insert(binding);
            let binding_ty = binding_type(db, binding);
            Some(narrowing_constraint.narrow(db, binding_ty, binding.place(db)))
        },
    );

    let place = if let Some(first) = types.next() {
        let ty = if let Some(second) = types.next() {
            let mut builder = PublicTypeBuilder::new(db);
            builder.add(first);
            builder.add(second);

            for ty in types {
                builder.add(ty);
            }

            builder.build()
        } else {
            first
        };

        let boundness = match boundness_analysis {
            BoundnessAnalysis::AssumeBound => Definedness::AlwaysDefined,
            BoundnessAnalysis::BasedOnUnboundVisibility => match unbound_visibility() {
                Some(Truthiness::AlwaysTrue) => {
                    unreachable!(
                        "If we have at least one binding, the implicit `unbound` binding should not be definitely visible"
                    )
                }
                Some(Truthiness::AlwaysFalse) | None => Definedness::AlwaysDefined,
                Some(Truthiness::Ambiguous) => Definedness::PossiblyUndefined,
            },
        };

        match deleted_reachability {
            Truthiness::AlwaysFalse => {
                Place::Defined(ty, TypeOrigin::Inferred, boundness, Widening::None)
            }
            Truthiness::AlwaysTrue => Place::Undefined,
            Truthiness::Ambiguous => Place::Defined(
                ty,
                TypeOrigin::Inferred,
                Definedness::PossiblyUndefined,
                Widening::None,
            ),
        }
    } else {
        Place::Undefined
    };

    PlaceWithDefinition {
        place,
        first_definition,
    }
}

pub(super) struct PlaceWithDefinition<'db> {
    pub(super) place: Place<'db>,
    pub(super) first_definition: Option<Definition<'db>>,
}

/// Accumulates types from multiple bindings or declarations, and eventually builds a
/// union type from them.
///
/// `@overload`ed function literal types are discarded if they are immediately followed
/// by their implementation. This is to ensure that we do not merge all of them into the
/// union type. The last one will include the other overloads already.
struct PublicTypeBuilder<'db> {
    db: &'db dyn Db,
    queue: Option<Type<'db>>,
    builder: UnionBuilder<'db>,
}

impl<'db> PublicTypeBuilder<'db> {
    fn new(db: &'db dyn Db) -> Self {
        PublicTypeBuilder {
            db,
            queue: None,
            builder: UnionBuilder::new(db),
        }
    }

    fn add_to_union(&mut self, element: Type<'db>) {
        self.builder.add_in_place(element);
    }

    fn drain_queue(&mut self) {
        if let Some(queued_element) = self.queue.take() {
            self.add_to_union(queued_element);
        }
    }

    fn add(&mut self, element: Type<'db>) -> bool {
        match element {
            Type::FunctionLiteral(function) => {
                if function
                    .literal(self.db)
                    .last_definition(self.db)
                    .is_overload(self.db)
                {
                    self.queue = Some(element);
                    false
                } else {
                    self.queue = None;
                    self.add_to_union(element);
                    true
                }
            }
            _ => {
                self.drain_queue();
                self.add_to_union(element);
                true
            }
        }
    }

    fn build(mut self) -> Type<'db> {
        self.drain_queue();
        self.builder.build()
    }
}

/// Accumulates multiple (potentially conflicting) declared types and type qualifiers,
/// and eventually builds a union from them.
struct DeclaredTypeBuilder<'db> {
    inner: PublicTypeBuilder<'db>,
    qualifiers: TypeQualifiers,
    first_type: Option<Type<'db>>,
    conflicting_types: FxOrderSet<Type<'db>>,
}

impl<'db> DeclaredTypeBuilder<'db> {
    fn new(db: &'db dyn Db) -> Self {
        DeclaredTypeBuilder {
            inner: PublicTypeBuilder::new(db),
            qualifiers: TypeQualifiers::empty(),
            first_type: None,
            conflicting_types: FxOrderSet::default(),
        }
    }

    fn add(&mut self, element: TypeAndQualifiers<'db>) {
        let element_ty = element.inner_type();

        if self.inner.add(element_ty) {
            if let Some(first_ty) = self.first_type {
                if !first_ty.is_equivalent_to(self.inner.db, element_ty) {
                    self.conflicting_types.insert(element_ty);
                }
            } else {
                self.first_type = Some(element_ty);
            }
        }

        self.qualifiers = self.qualifiers.union(element.qualifiers());
    }

    fn build(mut self) -> DeclaredTypeAndConflictingTypes<'db> {
        let type_and_quals =
            TypeAndQualifiers::new(self.inner.build(), TypeOrigin::Declared, self.qualifiers);
        if self.conflicting_types.is_empty() {
            (type_and_quals, None)
        } else {
            self.conflicting_types.insert_before(
                0,
                self.first_type
                    .expect("there must be a first type if there are conflicting types"),
            );
            (
                type_and_quals,
                Some(self.conflicting_types.into_boxed_slice()),
            )
        }
    }
}

/// Implementation of [`place_from_declarations`].
///
/// ## Implementation Note
/// This function gets called cross-module. It, therefore, shouldn't
/// access any AST nodes from the file containing the declarations.
fn place_from_declarations_impl<'db>(
    db: &'db dyn Db,
    declarations: DeclarationsIterator<'_, 'db>,
    requires_explicit_reexport: RequiresExplicitReExport,
) -> PlaceFromDeclarationsResult<'db> {
    let predicates = declarations.predicates;
    let reachability_constraints = declarations.reachability_constraints;
    let boundness_analysis = declarations.boundness_analysis;
    let mut declarations = declarations.peekable();
    let mut first_declaration = None;

    let is_non_exported = |declaration: Definition<'db>| {
        requires_explicit_reexport.is_yes() && !is_reexported(db, declaration)
    };

    let undeclared_reachability = match declarations.peek() {
        Some(DeclarationWithConstraint {
            declaration,
            reachability_constraint,
        }) if declaration.is_undefined_or(is_non_exported) => {
            reachability_constraints.evaluate(db, predicates, *reachability_constraint)
        }
        _ => Truthiness::AlwaysFalse,
    };

    let mut all_declarations_definitely_reachable = true;

    let mut types = declarations.filter_map(
        |DeclarationWithConstraint {
             declaration,
             reachability_constraint,
         }| {
            let DefinitionState::Defined(declaration) = declaration else {
                return None;
            };

            if is_non_exported(declaration) {
                return None;
            }

            first_declaration.get_or_insert(declaration);

            let static_reachability =
                reachability_constraints.evaluate(db, predicates, reachability_constraint);

            if static_reachability.is_always_false() {
                None
            } else {
                all_declarations_definitely_reachable =
                    all_declarations_definitely_reachable && static_reachability.is_always_true();

                Some(declaration_type(db, declaration))
            }
        },
    );

    if let Some(first) = types.next() {
        let (declared, conflicting) = if let Some(second) = types.next() {
            let mut builder = DeclaredTypeBuilder::new(db);
            builder.add(first);
            builder.add(second);
            for element in types {
                builder.add(element);
            }
            builder.build()
        } else {
            (first, None)
        };

        let boundness = match boundness_analysis {
            BoundnessAnalysis::AssumeBound => {
                if all_declarations_definitely_reachable {
                    Definedness::AlwaysDefined
                } else {
                    // For declarations, it is important to consider the possibility that they might only
                    // be bound in one control flow path, while the other path contains a binding. In order
                    // to even consider the bindings as well in `place_by_id`, we return `PossiblyUnbound`
                    // here.
                    Definedness::PossiblyUndefined
                }
            }
            BoundnessAnalysis::BasedOnUnboundVisibility => match undeclared_reachability {
                Truthiness::AlwaysTrue => {
                    unreachable!(
                        "If we have at least one declaration, the implicit `unbound` binding should not be definitely visible"
                    )
                }
                Truthiness::AlwaysFalse => Definedness::AlwaysDefined,
                Truthiness::Ambiguous => Definedness::PossiblyUndefined,
            },
        };

        let place_and_quals = Place::Defined(
            declared.inner_type(),
            TypeOrigin::Declared,
            boundness,
            Widening::None,
        )
        .with_qualifiers(declared.qualifiers());

        if let Some(conflicting) = conflicting {
            PlaceFromDeclarationsResult::conflict(place_and_quals, conflicting, first_declaration)
        } else {
            PlaceFromDeclarationsResult {
                place_and_quals,
                conflicting_types: None,
                first_declaration,
            }
        }
    } else {
        PlaceFromDeclarationsResult {
            place_and_quals: Place::Undefined.into(),
            conflicting_types: None,
            first_declaration: None,
        }
    }
}

// Returns `true` if the `definition` is re-exported.
//
// This will first check if the definition is using the "redundant alias" pattern like `import foo
// as foo` or `from foo import bar as bar`. If it's not, it will check whether the symbol is being
// exported via `__all__`.
fn is_reexported(db: &dyn Db, definition: Definition<'_>) -> bool {
    // This information is computed by the semantic index builder.
    if definition.is_reexported(db) {
        return true;
    }
    // At this point, the definition should either be an `import` or `from ... import` statement.
    // This is because the default value of `is_reexported` is `true` for any other kind of
    // definition.
    let Some(all_names) = dunder_all_names(db, definition.file(db)) else {
        return false;
    };
    let table = place_table(db, definition.scope(db));
    let symbol_id = definition.place(db).expect_symbol();
    let symbol_name = table.symbol(symbol_id).name();
    all_names.contains(symbol_name)
}

mod implicit_globals {
    use ruff_python_ast as ast;
    use ruff_python_ast::name::Name;

    use crate::Program;
    use crate::db::Db;
    use crate::place::{Definedness, PlaceAndQualifiers, TypeOrigin};
    use crate::semantic_index::symbol::Symbol;
    use crate::semantic_index::{place_table, use_def_map};
    use crate::types::{KnownClass, MemberLookupPolicy, Parameter, Parameters, Signature, Type};
    use ruff_python_ast::PythonVersion;

    use super::{Place, Widening, place_from_declarations};

    pub(crate) fn module_type_implicit_global_declaration<'db>(
        db: &'db dyn Db,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        if !module_type_symbols(db)
            .iter()
            .any(|module_type_member| module_type_member == name)
        {
            return Place::Undefined.into();
        }
        let Type::ClassLiteral(module_type_class) = KnownClass::ModuleType.to_class_literal(db)
        else {
            return Place::Undefined.into();
        };
        let module_type_scope = module_type_class.body_scope(db);
        let place_table = place_table(db, module_type_scope);
        let Some(symbol_id) = place_table.symbol_id(name) else {
            return Place::Undefined.into();
        };
        place_from_declarations(
            db,
            use_def_map(db, module_type_scope).end_of_scope_symbol_declarations(symbol_id),
        )
        .ignore_conflicting_declarations()
    }

    /// Looks up the type of an "implicit global symbol". Returns [`Place::Undefined`] if
    /// `name` is not present as an implicit symbol in module-global namespaces.
    ///
    /// Implicit global symbols are symbols such as `__doc__`, `__name__`, and `__file__`
    /// that are implicitly defined in every module's global scope. Because their type is
    /// always the same, we simply look these up as instance attributes on `types.ModuleType`.
    ///
    /// Note that this function should only be used as a fallback if a symbol is being looked
    /// up in the global scope **from within the same file**. If the symbol is being looked up
    /// from outside the file (e.g. via imports), use [`super::imported_symbol`] (or fallback logic
    /// like the logic used in that function) instead. The reason is that this function returns
    /// [`Place::Undefined`] for `__init__` and `__dict__` (which cannot be found in globals if
    /// the lookup is being done from the same file) -- but these symbols *are* available in the
    /// global scope if they're being imported **from a different file**.
    pub(crate) fn module_type_implicit_global_symbol<'db>(
        db: &'db dyn Db,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        match name {
            // We special-case `__file__` here because we know that for an internal implicit global
            // lookup in a Python module, it is always a string, even though typeshed says `str |
            // None`.
            "__file__" => Place::bound(KnownClass::Str.to_instance(db)).into(),

            "__builtins__" => Place::bound(Type::any()).into(),

            "__debug__" => Place::bound(KnownClass::Bool.to_instance(db)).into(),

            // Created lazily by the warnings machinery; may be absent.
            // Model as possibly-unbound to avoid false negatives.
            "__warningregistry__" => Place::Defined(
                KnownClass::Dict
                    .to_specialized_instance(db, [Type::any(), KnownClass::Int.to_instance(db)]),
                TypeOrigin::Inferred,
                Definedness::PossiblyUndefined,
                Widening::None,
            )
            .into(),

            // Marked as possibly-unbound as it is only present in the module namespace
            // if at least one global symbol is annotated in the module.
            "__annotate__" if Program::get(db).python_version(db) >= PythonVersion::PY314 => {
                let signature = Signature::new(
                    Parameters::new(
                        db,
                        [Parameter::positional_only(Some(Name::new_static("format")))
                            .with_annotated_type(KnownClass::Int.to_instance(db))],
                    ),
                    Some(KnownClass::Dict.to_specialized_instance(
                        db,
                        [KnownClass::Str.to_instance(db), Type::any()],
                    )),
                );
                Place::Defined(
                    Type::function_like_callable(db, signature),
                    TypeOrigin::Inferred,
                    Definedness::PossiblyUndefined,
                    Widening::None,
                )
                .into()
            }

            // In general we wouldn't check to see whether a symbol exists on a class before doing the
            // `.member()` call on the instance type -- we'd just do the `.member`() call on the instance
            // type, since it has the same end result. The reason to only call `.member()` on `ModuleType`
            // when absolutely necessary is that this function is used in a very hot path (name resolution
            // in `infer.rs`). We use less idiomatic (and much more verbose) code here as a micro-optimisation.
            _ if module_type_symbols(db)
                .iter()
                .any(|module_type_member| &**module_type_member == name) =>
            {
                KnownClass::ModuleType
                    .to_instance(db)
                    .member_lookup_with_policy(
                        db,
                        name.into(),
                        MemberLookupPolicy::NO_GETATTR_LOOKUP,
                    )
            }

            _ => Place::Undefined.into(),
        }
    }

    /// An internal micro-optimisation for `module_type_implicit_global_symbol`.
    ///
    /// This function returns a list of the symbols that typeshed declares in the
    /// body scope of the stub for the class `types.ModuleType`.
    ///
    /// The returned list excludes the attributes `__dict__` and `__init__`. These are very
    /// special members that can be accessed as attributes on the module when imported,
    /// but cannot be accessed as globals *inside* the module.
    ///
    /// The list also excludes `__getattr__`. `__getattr__` is even more special: it doesn't
    /// exist at runtime, but typeshed includes it to reduce false positives associated with
    /// functions that dynamically import modules and return `Instance(types.ModuleType)`.
    /// We should ignore it for any known module-literal type.
    ///
    /// Conceptually this function could be a `Set` rather than a list,
    /// but the number of symbols declared in this scope is likely to be very small,
    /// so the cost of hashing the names is likely to be more expensive than it's worth.
    #[salsa::tracked(
        returns(deref),
        cycle_initial=module_type_symbols_initial,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn module_type_symbols<'db>(db: &'db dyn Db) -> smallvec::SmallVec<[ast::name::Name; 8]> {
        let Some(module_type) = KnownClass::ModuleType
            .to_class_literal(db)
            .as_class_literal()
        else {
            // The most likely way we get here is if a user specified a `--custom-typeshed-dir`
            // without a `types.pyi` stub in the `stdlib/` directory
            return smallvec::SmallVec::default();
        };

        let module_type_scope = module_type.body_scope(db);
        let module_type_symbol_table = place_table(db, module_type_scope);

        module_type_symbol_table
            .symbols()
            .filter(|symbol| symbol.is_declared())
            .map(Symbol::name)
            .filter(|symbol_name| {
                !matches!(
                    symbol_name.as_str(),
                    "__dict__" | "__getattr__" | "__init__"
                )
            })
            .cloned()
            .collect()
    }

    fn module_type_symbols_initial(
        _db: &dyn Db,
        _id: salsa::Id,
    ) -> smallvec::SmallVec<[ast::name::Name; 8]> {
        smallvec::SmallVec::default()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::db::tests::setup_db;

        #[test]
        fn module_type_symbols_includes_declared_types_but_not_referenced_types() {
            let db = setup_db();
            let symbol_names = module_type_symbols(&db);

            let dunder_name_symbol_name = ast::name::Name::new_static("__name__");
            assert!(symbol_names.contains(&dunder_name_symbol_name));

            let property_symbol_name = ast::name::Name::new_static("property");
            assert!(!symbol_names.contains(&property_symbol_name));
        }
    }
}

/// Looks up the type of an "implicit class body symbol". Returns [`Place::Undefined`] if
/// `name` is not present as an implicit symbol in class bodies.
///
/// Implicit class body symbols are symbols such as `__qualname__`, `__module__`, `__doc__`,
/// and `__firstlineno__` that Python implicitly makes available inside a class body during
/// class creation.
///
/// See <https://docs.python.org/3/reference/datamodel.html#creating-the-class-object>
pub(crate) fn class_body_implicit_symbol<'db>(
    db: &'db dyn Db,
    name: &str,
) -> PlaceAndQualifiers<'db> {
    match name {
        "__qualname__" => Place::bound(KnownClass::Str.to_instance(db)).into(),
        "__module__" => Place::bound(KnownClass::Str.to_instance(db)).into(),
        // __doc__ is `str` if there's a docstring, `None` if there isn't
        "__doc__" => Place::bound(UnionType::from_elements(
            db,
            [KnownClass::Str.to_instance(db), Type::none(db)],
        ))
        .into(),
        // __firstlineno__ was added in Python 3.13
        "__firstlineno__" if Program::get(db).python_version(db) >= PythonVersion::PY313 => {
            Place::bound(KnownClass::Int.to_instance(db)).into()
        }
        _ => Place::Undefined.into(),
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RequiresExplicitReExport {
    Yes,
    No,
}

impl RequiresExplicitReExport {
    const fn is_yes(self) -> bool {
        matches!(self, RequiresExplicitReExport::Yes)
    }
}

/// Specifies which definitions should be considered when looking up a place.
///
/// In the example below, the `EndOfScope` variant would consider the `x = 2` and `x = 3` definitions,
/// while the `AllReachable` variant would also consider the `x = 1` definition.
/// ```py
/// def _():
///     x = 1
///
///     x = 2
///
///     if flag():
///         x = 3
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
pub(crate) enum ConsideredDefinitions {
    /// Consider only the definitions that are "live" at the end of the scope, i.e. those
    /// that have not been shadowed or deleted.
    EndOfScope,
    /// Consider all definitions that are reachable from the start of the scope.
    AllReachable,
}

/// Specifies how the boundness of a place should be determined.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
pub(crate) enum BoundnessAnalysis {
    /// The place is always considered bound.
    AssumeBound,
    /// The boundness of the place is determined based on the visibility of the implicit
    /// `unbound` binding. In the example below, when analyzing the visibility of the
    /// `x = <unbound>` binding from the position of the end of the scope, it would be
    /// `Truthiness::Ambiguous`, because it could either be visible or not, depending on the
    /// `flag()` return value. This would result in a `Definedness::PossiblyUndefined` for `x`.
    ///
    /// ```py
    /// x = <unbound>
    ///
    /// if flag():
    ///     x = 1
    /// ```
    BasedOnUnboundVisibility,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::setup_db;

    #[test]
    fn test_symbol_or_fall_back_to() {
        use Definedness::{AlwaysDefined, PossiblyUndefined};
        use TypeOrigin::Inferred;

        let db = setup_db();
        let ty1 = Type::IntLiteral(1);
        let ty2 = Type::IntLiteral(2);

        let unbound = || Place::Undefined.with_qualifiers(TypeQualifiers::empty());

        let possibly_unbound_ty1 = || {
            Place::Defined(ty1, Inferred, PossiblyUndefined, Widening::None)
                .with_qualifiers(TypeQualifiers::empty())
        };
        let possibly_unbound_ty2 = || {
            Place::Defined(ty2, Inferred, PossiblyUndefined, Widening::None)
                .with_qualifiers(TypeQualifiers::empty())
        };

        let bound_ty1 = || {
            Place::Defined(ty1, Inferred, AlwaysDefined, Widening::None)
                .with_qualifiers(TypeQualifiers::empty())
        };
        let bound_ty2 = || {
            Place::Defined(ty2, Inferred, AlwaysDefined, Widening::None)
                .with_qualifiers(TypeQualifiers::empty())
        };

        // Start from an unbound symbol
        assert_eq!(unbound().or_fall_back_to(&db, unbound), unbound());
        assert_eq!(
            unbound().or_fall_back_to(&db, possibly_unbound_ty1),
            possibly_unbound_ty1()
        );
        assert_eq!(unbound().or_fall_back_to(&db, bound_ty1), bound_ty1());

        // Start from a possibly unbound symbol
        assert_eq!(
            possibly_unbound_ty1().or_fall_back_to(&db, unbound),
            possibly_unbound_ty1()
        );
        assert_eq!(
            possibly_unbound_ty1().or_fall_back_to(&db, possibly_unbound_ty2),
            Place::Defined(
                UnionType::from_elements(&db, [ty1, ty2]),
                Inferred,
                PossiblyUndefined,
                Widening::None
            )
            .into()
        );
        assert_eq!(
            possibly_unbound_ty1().or_fall_back_to(&db, bound_ty2),
            Place::Defined(
                UnionType::from_elements(&db, [ty1, ty2]),
                Inferred,
                AlwaysDefined,
                Widening::None
            )
            .into()
        );

        // Start from a definitely bound symbol
        assert_eq!(bound_ty1().or_fall_back_to(&db, unbound), bound_ty1());
        assert_eq!(
            bound_ty1().or_fall_back_to(&db, possibly_unbound_ty2),
            bound_ty1()
        );
        assert_eq!(bound_ty1().or_fall_back_to(&db, bound_ty2), bound_ty1());
    }

    #[track_caller]
    fn assert_bound_string_symbol<'db>(db: &'db dyn Db, symbol: Place<'db>) {
        assert!(matches!(
            symbol,
            Place::Defined(Type::NominalInstance(_), _, Definedness::AlwaysDefined, _)
        ));
        assert_eq!(symbol.expect_type(), KnownClass::Str.to_instance(db));
    }

    #[test]
    fn implicit_builtin_globals() {
        let db = setup_db();
        assert_bound_string_symbol(&db, builtins_symbol(&db, "__name__").place);
    }

    #[test]
    fn implicit_typing_globals() {
        let db = setup_db();
        assert_bound_string_symbol(&db, typing_symbol(&db, "__name__").place);
    }

    #[test]
    fn implicit_typing_extensions_globals() {
        let db = setup_db();
        assert_bound_string_symbol(&db, typing_extensions_symbol(&db, "__name__").place);
    }

    #[test]
    fn implicit_sys_globals() {
        let db = setup_db();
        assert_bound_string_symbol(
            &db,
            known_module_symbol(&db, KnownModule::Sys, "__name__").place,
        );
    }
}
