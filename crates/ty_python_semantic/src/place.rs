use ruff_db::files::File;

use crate::dunder_all::dunder_all_names;
use crate::module_resolver::file_to_module;
use crate::semantic_index::definition::{Definition, DefinitionState};
use crate::semantic_index::place::{PlaceExpr, ScopeId, ScopedPlaceId};
use crate::semantic_index::{
    BindingWithConstraints, BindingWithConstraintsIterator, DeclarationsIterator, place_table,
};
use crate::semantic_index::{DeclarationWithConstraint, global_scope, use_def_map};
use crate::types::{
    KnownClass, Truthiness, Type, TypeAndQualifiers, TypeQualifiers, UnionBuilder, UnionType,
    binding_type, declaration_type, todo_type,
};
use crate::{Db, KnownModule, Program, resolve_module};

pub(crate) use implicit_globals::{
    module_type_implicit_global_declaration, module_type_implicit_global_symbol,
};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub(crate) enum Boundness {
    Bound,
    PossiblyUnbound,
}

impl Boundness {
    pub(crate) const fn max(self, other: Self) -> Self {
        match (self, other) {
            (Boundness::Bound, _) | (_, Boundness::Bound) => Boundness::Bound,
            (Boundness::PossiblyUnbound, Boundness::PossiblyUnbound) => Boundness::PossiblyUnbound,
        }
    }
}

/// The result of a place lookup, which can either be a (possibly unbound) type
/// or a completely unbound place.
///
/// Consider this example:
/// ```py
/// bound = 1
///
/// if flag:
///     possibly_unbound = 2
/// ```
///
/// If we look up places in this scope, we would get the following results:
/// ```rs
/// bound:             Place::Type(Type::IntLiteral(1), Boundness::Bound),
/// possibly_unbound:  Place::Type(Type::IntLiteral(2), Boundness::PossiblyUnbound),
/// non_existent:      Place::Unbound,
/// ```
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub(crate) enum Place<'db> {
    Type(Type<'db>, Boundness),
    Unbound,
}

impl<'db> Place<'db> {
    /// Constructor that creates a `Place` with boundness [`Boundness::Bound`].
    pub(crate) fn bound(ty: impl Into<Type<'db>>) -> Self {
        Place::Type(ty.into(), Boundness::Bound)
    }

    pub(crate) fn possibly_unbound(ty: impl Into<Type<'db>>) -> Self {
        Place::Type(ty.into(), Boundness::PossiblyUnbound)
    }

    /// Constructor that creates a [`Place`] with a [`crate::types::TodoType`] type
    /// and boundness [`Boundness::Bound`].
    #[allow(unused_variables)] // Only unused in release builds
    pub(crate) fn todo(message: &'static str) -> Self {
        Place::Type(todo_type!(message), Boundness::Bound)
    }

    pub(crate) fn is_unbound(&self) -> bool {
        matches!(self, Place::Unbound)
    }

    /// Returns the type of the place, ignoring possible unboundness.
    ///
    /// If the place is *definitely* unbound, this function will return `None`. Otherwise,
    /// if there is at least one control-flow path where the place is bound, return the type.
    pub(crate) fn ignore_possibly_unbound(&self) -> Option<Type<'db>> {
        match self {
            Place::Type(ty, _) => Some(*ty),
            Place::Unbound => None,
        }
    }

    #[cfg(test)]
    #[track_caller]
    pub(crate) fn expect_type(self) -> Type<'db> {
        self.ignore_possibly_unbound()
            .expect("Expected a (possibly unbound) type, not an unbound place")
    }

    #[must_use]
    pub(crate) fn map_type(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Place<'db> {
        match self {
            Place::Type(ty, boundness) => Place::Type(f(ty), boundness),
            Place::Unbound => Place::Unbound,
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
            Place::Type(Type::Union(union), boundness) => union.map_with_boundness(db, |elem| {
                Place::Type(*elem, boundness).try_call_dunder_get(db, owner)
            }),

            Place::Type(Type::Intersection(intersection), boundness) => intersection
                .map_with_boundness(db, |elem| {
                    Place::Type(*elem, boundness).try_call_dunder_get(db, owner)
                }),

            Place::Type(self_ty, boundness) => {
                if let Some((dunder_get_return_ty, _)) =
                    self_ty.try_call_dunder_get(db, Type::none(db), owner)
                {
                    Place::Type(dunder_get_return_ty, boundness)
                } else {
                    self
                }
            }

            Place::Unbound => Place::Unbound,
        }
    }
}

impl<'db> From<LookupResult<'db>> for PlaceAndQualifiers<'db> {
    fn from(value: LookupResult<'db>) -> Self {
        match value {
            Ok(type_and_qualifiers) => {
                Place::Type(type_and_qualifiers.inner_type(), Boundness::Bound)
                    .with_qualifiers(type_and_qualifiers.qualifiers())
            }
            Err(LookupError::Unbound(qualifiers)) => Place::Unbound.with_qualifiers(qualifiers),
            Err(LookupError::PossiblyUnbound(type_and_qualifiers)) => {
                Place::Type(type_and_qualifiers.inner_type(), Boundness::PossiblyUnbound)
                    .with_qualifiers(type_and_qualifiers.qualifiers())
            }
        }
    }
}

/// Possible ways in which a place lookup can (possibly or definitely) fail.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum LookupError<'db> {
    Unbound(TypeQualifiers),
    PossiblyUnbound(TypeAndQualifiers<'db>),
}

impl<'db> LookupError<'db> {
    /// Fallback (wholly or partially) to `fallback` to create a new [`LookupResult`].
    pub(crate) fn or_fall_back_to(
        self,
        db: &'db dyn Db,
        fallback: PlaceAndQualifiers<'db>,
    ) -> LookupResult<'db> {
        let fallback = fallback.into_lookup_result();
        match (&self, &fallback) {
            (LookupError::Unbound(_), _) => fallback,
            (LookupError::PossiblyUnbound { .. }, Err(LookupError::Unbound(_))) => Err(self),
            (LookupError::PossiblyUnbound(ty), Ok(ty2)) => Ok(TypeAndQualifiers::new(
                UnionType::from_elements(db, [ty.inner_type(), ty2.inner_type()]),
                ty.qualifiers().union(ty2.qualifiers()),
            )),
            (LookupError::PossiblyUnbound(ty), Err(LookupError::PossiblyUnbound(ty2))) => {
                Err(LookupError::PossiblyUnbound(TypeAndQualifiers::new(
                    UnionType::from_elements(db, [ty.inner_type(), ty2.inner_type()]),
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
) -> PlaceAndQualifiers<'db> {
    symbol_impl(db, scope, name, RequiresExplicitReExport::No)
}

/// Infer the public type of a place (its type as seen from outside its scope) in the given
/// `scope`.
pub(crate) fn place<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    expr: &PlaceExpr,
) -> PlaceAndQualifiers<'db> {
    place_impl(db, scope, expr, RequiresExplicitReExport::No)
}

/// Infer the public type of a class symbol (its type as seen from outside its scope) in the given
/// `scope`.
pub(crate) fn class_symbol<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
) -> PlaceAndQualifiers<'db> {
    place_table(db, scope)
        .place_id_by_name(name)
        .map(|symbol| {
            let symbol_and_quals = place_by_id(db, scope, symbol, RequiresExplicitReExport::No);

            if symbol_and_quals.is_class_var() {
                // For declared class vars we do not need to check if they have bindings,
                // we just trust the declaration.
                return symbol_and_quals;
            }

            if let PlaceAndQualifiers {
                place: Place::Type(ty, _),
                qualifiers,
            } = symbol_and_quals
            {
                // Otherwise, we need to check if the symbol has bindings
                let use_def = use_def_map(db, scope);
                let bindings = use_def.public_bindings(symbol);
                let inferred = place_from_bindings_impl(db, bindings, RequiresExplicitReExport::No);

                // TODO: we should not need to calculate inferred type second time. This is a temporary
                // solution until the notion of Boundness and Declaredness is split. See #16036, #16264
                match inferred {
                    Place::Unbound => Place::Unbound.with_qualifiers(qualifiers),
                    Place::Type(_, boundness) => {
                        Place::Type(ty, boundness).with_qualifiers(qualifiers)
                    }
                }
            } else {
                Place::Unbound.into()
            }
        })
        .unwrap_or_default()
}

/// Infers the public type of an explicit module-global symbol as seen from within the same file.
///
/// Note that all global scopes also include various "implicit globals" such as `__name__`,
/// `__doc__` and `__file__`. This function **does not** consider those symbols; it will return
/// `Place::Unbound` for them. Use the (currently test-only) `global_symbol` query to also include
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
        if file.is_stub(db.upcast()) {
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
    symbol_impl(db, global_scope(db, file), name, requires_explicit_reexport).or_fall_back_to(
        db,
        || {
            if name == "__getattr__" {
                Place::Unbound.into()
            } else if name == "__builtins__" {
                Place::bound(Type::any()).into()
            } else {
                KnownClass::ModuleType.to_instance(db).member(db, name)
            }
        },
    )
}

/// Lookup the type of `symbol` in the builtins namespace.
///
/// Returns `Place::Unbound` if the `builtins` module isn't available for some reason.
///
/// Note that this function is only intended for use in the context of the builtins *namespace*
/// and should not be used when a symbol is being explicitly imported from the `builtins` module
/// (e.g. `from builtins import int`).
pub(crate) fn builtins_symbol<'db>(db: &'db dyn Db, symbol: &str) -> PlaceAndQualifiers<'db> {
    resolve_module(db, &KnownModule::Builtins.name())
        .and_then(|module| {
            let file = module.file()?;
            Some(
                symbol_impl(
                    db,
                    global_scope(db, file),
                    symbol,
                    RequiresExplicitReExport::Yes,
                )
                .or_fall_back_to(db, || {
                    // We're looking up in the builtins namespace and not the module, so we should
                    // do the normal lookup in `types.ModuleType` and not the special one as in
                    // `imported_symbol`.
                    module_type_implicit_global_symbol(db, symbol)
                }),
            )
        })
        .unwrap_or_default()
}

/// Lookup the type of `symbol` in a given known module.
///
/// Returns `Place::Unbound` if the given known module cannot be resolved for some reason.
pub(crate) fn known_module_symbol<'db>(
    db: &'db dyn Db,
    known_module: KnownModule,
    symbol: &str,
) -> PlaceAndQualifiers<'db> {
    resolve_module(db, &known_module.name())
        .and_then(|module| {
            let file = module.file()?;
            Some(imported_symbol(db, file, symbol, None))
        })
        .unwrap_or_default()
}

/// Lookup the type of `symbol` in the `typing` module namespace.
///
/// Returns `Place::Unbound` if the `typing` module isn't available for some reason.
#[inline]
#[cfg(test)]
pub(crate) fn typing_symbol<'db>(db: &'db dyn Db, symbol: &str) -> PlaceAndQualifiers<'db> {
    known_module_symbol(db, KnownModule::Typing, symbol)
}

/// Lookup the type of `symbol` in the `typing_extensions` module namespace.
///
/// Returns `Place::Unbound` if the `typing_extensions` module isn't available for some reason.
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
    let module = resolve_module(db, &core_module.name())?;
    Some(global_scope(db, module.file()?))
}

/// Infer the combined type from an iterator of bindings, and return it
/// together with boundness information in a [`Place`].
///
/// The type will be a union if there are multiple bindings with different types.
pub(super) fn place_from_bindings<'db>(
    db: &'db dyn Db,
    bindings_with_constraints: BindingWithConstraintsIterator<'_, 'db>,
) -> Place<'db> {
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

/// The result of looking up a declared type from declarations; see [`place_from_declarations`].
pub(crate) type PlaceFromDeclarationsResult<'db> =
    Result<PlaceAndQualifiers<'db>, (TypeAndQualifiers<'db>, Box<[Type<'db>]>)>;

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
/// the type `int`, a "declaredness" of [`Boundness::PossiblyUnbound`], and the information
/// that this comes with a [`CLASS_VAR`] type qualifier.
///
/// [`CLASS_VAR`]: crate::types::TypeQualifiers::CLASS_VAR
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub(crate) struct PlaceAndQualifiers<'db> {
    pub(crate) place: Place<'db>,
    pub(crate) qualifiers: TypeQualifiers,
}

impl Default for PlaceAndQualifiers<'_> {
    fn default() -> Self {
        PlaceAndQualifiers {
            place: Place::Unbound,
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

    /// Returns `true` if the place has a `ClassVar` type qualifier.
    pub(crate) fn is_class_var(&self) -> bool {
        self.qualifiers.contains(TypeQualifiers::CLASS_VAR)
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

    /// Transform place and qualifiers into a [`LookupResult`],
    /// a [`Result`] type in which the `Ok` variant represents a definitely bound place
    /// and the `Err` variant represents a place that is either definitely or possibly unbound.
    pub(crate) fn into_lookup_result(self) -> LookupResult<'db> {
        match self {
            PlaceAndQualifiers {
                place: Place::Type(ty, Boundness::Bound),
                qualifiers,
            } => Ok(TypeAndQualifiers::new(ty, qualifiers)),
            PlaceAndQualifiers {
                place: Place::Type(ty, Boundness::PossiblyUnbound),
                qualifiers,
            } => Err(LookupError::PossiblyUnbound(TypeAndQualifiers::new(
                ty, qualifiers,
            ))),
            PlaceAndQualifiers {
                place: Place::Unbound,
                qualifiers,
            } => Err(LookupError::Unbound(qualifiers)),
        }
    }

    /// Safely unwrap the place and the qualifiers into a [`TypeQualifiers`].
    ///
    /// If the place is definitely unbound or possibly unbound, it will be transformed into a
    /// [`LookupError`] and `diagnostic_fn` will be applied to the error value before returning
    /// the result of `diagnostic_fn` (which will be a [`TypeQualifiers`]). This allows the caller
    /// to ensure that a diagnostic is emitted if the place is possibly or definitely unbound.
    pub(crate) fn unwrap_with_diagnostic(
        self,
        diagnostic_fn: impl FnOnce(LookupError<'db>) -> TypeAndQualifiers<'db>,
    ) -> TypeAndQualifiers<'db> {
        self.into_lookup_result().unwrap_or_else(diagnostic_fn)
    }

    /// Fallback (partially or fully) to another place if `self` is partially or fully unbound.
    ///
    /// 1. If `self` is definitely bound, return `self` without evaluating `fallback_fn()`.
    /// 2. Else, evaluate `fallback_fn()`:
    ///    1. If `self` is definitely unbound, return the result of `fallback_fn()`.
    ///    2. Else, if `fallback` is definitely unbound, return `self`.
    ///    3. Else, if `self` is possibly unbound and `fallback` is definitely bound,
    ///       return `Place(<union of self-type and fallback-type>, Boundness::Bound)`
    ///    4. Else, if `self` is possibly unbound and `fallback` is possibly unbound,
    ///       return `Place(<union of self-type and fallback-type>, Boundness::PossiblyUnbound)`
    #[must_use]
    pub(crate) fn or_fall_back_to(
        self,
        db: &'db dyn Db,
        fallback_fn: impl FnOnce() -> PlaceAndQualifiers<'db>,
    ) -> Self {
        self.into_lookup_result()
            .or_else(|lookup_error| lookup_error.or_fall_back_to(db, fallback_fn()))
            .into()
    }
}

impl<'db> From<Place<'db>> for PlaceAndQualifiers<'db> {
    fn from(place: Place<'db>) -> Self {
        place.with_qualifiers(TypeQualifiers::empty())
    }
}

fn place_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &PlaceAndQualifiers<'db>,
    _count: u32,
    _scope: ScopeId<'db>,
    _place_id: ScopedPlaceId,
    _requires_explicit_reexport: RequiresExplicitReExport,
) -> salsa::CycleRecoveryAction<PlaceAndQualifiers<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn place_cycle_initial<'db>(
    _db: &'db dyn Db,
    _scope: ScopeId<'db>,
    _place_id: ScopedPlaceId,
    _requires_explicit_reexport: RequiresExplicitReExport,
) -> PlaceAndQualifiers<'db> {
    Place::bound(Type::Never).into()
}

#[salsa::tracked(cycle_fn=place_cycle_recover, cycle_initial=place_cycle_initial)]
fn place_by_id<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    place_id: ScopedPlaceId,
    requires_explicit_reexport: RequiresExplicitReExport,
) -> PlaceAndQualifiers<'db> {
    let use_def = use_def_map(db, scope);

    // If the place is declared, the public type is based on declarations; otherwise, it's based
    // on inference from bindings.

    let declarations = use_def.public_declarations(place_id);
    let declared = place_from_declarations_impl(db, declarations, requires_explicit_reexport);

    match declared {
        // Place is declared, trust the declared type
        Ok(
            place_and_quals @ PlaceAndQualifiers {
                place: Place::Type(_, Boundness::Bound),
                qualifiers: _,
            },
        ) => place_and_quals,
        // Place is possibly declared
        Ok(PlaceAndQualifiers {
            place: Place::Type(declared_ty, Boundness::PossiblyUnbound),
            qualifiers,
        }) => {
            let bindings = use_def.public_bindings(place_id);
            let inferred = place_from_bindings_impl(db, bindings, requires_explicit_reexport);

            let place = match inferred {
                // Place is possibly undeclared and definitely unbound
                Place::Unbound => {
                    // TODO: We probably don't want to report `Bound` here. This requires a bit of
                    // design work though as we might want a different behavior for stubs and for
                    // normal modules.
                    Place::Type(declared_ty, Boundness::Bound)
                }
                // Place is possibly undeclared and (possibly) bound
                Place::Type(inferred_ty, boundness) => Place::Type(
                    UnionType::from_elements(db, [inferred_ty, declared_ty]),
                    boundness,
                ),
            };

            PlaceAndQualifiers { place, qualifiers }
        }
        // Place is undeclared, return the union of `Unknown` with the inferred type
        Ok(PlaceAndQualifiers {
            place: Place::Unbound,
            qualifiers: _,
        }) => {
            let bindings = use_def.public_bindings(place_id);
            let inferred = place_from_bindings_impl(db, bindings, requires_explicit_reexport);

            // `__slots__` is a symbol with special behavior in Python's runtime. It can be
            // modified externally, but those changes do not take effect. We therefore issue
            // a diagnostic if we see it being modified externally. In type inference, we
            // can assign a "narrow" type to it even if it is not *declared*. This means, we
            // do not have to call [`widen_type_for_undeclared_public_symbol`].
            //
            // `TYPE_CHECKING` is a special variable that should only be assigned `False`
            // at runtime, but is always considered `True` in type checking.
            // See mdtest/known_constants.md#user-defined-type_checking for details.
            let is_considered_non_modifiable = place_table(db, scope)
                .place_expr(place_id)
                .expr
                .is_name_and(|name| matches!(name, "__slots__" | "TYPE_CHECKING"));

            if scope.file(db).is_stub(db.upcast()) {
                // We generally trust module-level undeclared places in stubs and do not union
                // with `Unknown`. If we don't do this, simple aliases like `IOError = OSError` in
                // stubs would result in `IOError` being a union of `OSError` and `Unknown`, which
                // leads to all sorts of downstream problems. Similarly, type variables are often
                // defined as `_T = TypeVar("_T")`, without being declared.

                inferred.into()
            } else {
                widen_type_for_undeclared_public_symbol(db, inferred, is_considered_non_modifiable)
                    .into()
            }
        }
        // Place has conflicting declared types
        Err((declared, _)) => {
            // Intentionally ignore conflicting declared types; that's not our problem,
            // it's the problem of the module we are importing from.
            Place::bound(declared.inner_type()).with_qualifiers(declared.qualifiers())
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
) -> PlaceAndQualifiers<'db> {
    let _span = tracing::trace_span!("symbol", ?name).entered();

    if name == "platform"
        && file_to_module(db, scope.file(db))
            .is_some_and(|module| module.is_known(KnownModule::Sys))
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
        .place_id_by_name(name)
        .map(|symbol| place_by_id(db, scope, symbol, requires_explicit_reexport))
        .unwrap_or_default()
}

/// Implementation of [`place`].
fn place_impl<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    expr: &PlaceExpr,
    requires_explicit_reexport: RequiresExplicitReExport,
) -> PlaceAndQualifiers<'db> {
    let _span = tracing::trace_span!("place", ?expr).entered();

    place_table(db, scope)
        .place_id_by_expr(expr)
        .map(|place| place_by_id(db, scope, place, requires_explicit_reexport))
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
) -> Place<'db> {
    let predicates = bindings_with_constraints.predicates;
    let reachability_constraints = bindings_with_constraints.reachability_constraints;
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
    let unbound_reachability = || {
        unbound_reachability_constraint.map(|reachability_constraint| {
            reachability_constraints.evaluate(db, predicates, reachability_constraint)
        })
    };

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

                if unbound_reachability().is_none_or(Truthiness::is_always_false) {
                    return Some(Type::Never);
                }
                return None;
            }

            let binding_ty = binding_type(db, binding);
            Some(narrowing_constraint.narrow(db, binding_ty, binding.place(db)))
        },
    );

    if let Some(first) = types.next() {
        let boundness = match unbound_reachability() {
            Some(Truthiness::AlwaysTrue) => {
                unreachable!(
                    "If we have at least one binding, the implicit `unbound` binding should not be definitely visible"
                )
            }
            Some(Truthiness::AlwaysFalse) | None => Boundness::Bound,
            Some(Truthiness::Ambiguous) => Boundness::PossiblyUnbound,
        };

        let ty = if let Some(second) = types.next() {
            UnionType::from_elements(db, [first, second].into_iter().chain(types))
        } else {
            first
        };
        match deleted_reachability {
            Truthiness::AlwaysFalse => Place::Type(ty, boundness),
            Truthiness::AlwaysTrue => Place::Unbound,
            Truthiness::Ambiguous => Place::Type(ty, Boundness::PossiblyUnbound),
        }
    } else {
        Place::Unbound
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
    let mut declarations = declarations.peekable();

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

            let static_reachability =
                reachability_constraints.evaluate(db, predicates, reachability_constraint);

            if static_reachability.is_always_false() {
                None
            } else {
                Some(declaration_type(db, declaration))
            }
        },
    );

    if let Some(first) = types.next() {
        let mut conflicting: Vec<Type<'db>> = vec![];
        let declared = if let Some(second) = types.next() {
            let ty_first = first.inner_type();
            let mut qualifiers = first.qualifiers();

            let mut builder = UnionBuilder::new(db).add(ty_first);
            for other in std::iter::once(second).chain(types) {
                let other_ty = other.inner_type();
                if !ty_first.is_equivalent_to(db, other_ty) {
                    conflicting.push(other_ty);
                }
                builder = builder.add(other_ty);
                qualifiers = qualifiers.union(other.qualifiers());
            }
            TypeAndQualifiers::new(builder.build(), qualifiers)
        } else {
            first
        };
        if conflicting.is_empty() {
            let boundness = match undeclared_reachability {
                Truthiness::AlwaysTrue => {
                    unreachable!(
                        "If we have at least one declaration, the implicit `unbound` binding should not be definitely visible"
                    )
                }
                Truthiness::AlwaysFalse => Boundness::Bound,
                Truthiness::Ambiguous => Boundness::PossiblyUnbound,
            };

            Ok(
                Place::Type(declared.inner_type(), boundness)
                    .with_qualifiers(declared.qualifiers()),
            )
        } else {
            Err((
                declared,
                std::iter::once(first.inner_type())
                    .chain(conflicting)
                    .collect(),
            ))
        }
    } else {
        Ok(Place::Unbound.into())
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
    let symbol_name = table.place_expr(definition.place(db)).expect_name();
    all_names.contains(symbol_name)
}

mod implicit_globals {
    use ruff_python_ast as ast;

    use crate::db::Db;
    use crate::place::PlaceAndQualifiers;
    use crate::semantic_index::place::PlaceExpr;
    use crate::semantic_index::{self, place_table, use_def_map};
    use crate::types::{KnownClass, Type};

    use super::{Place, PlaceFromDeclarationsResult, place_from_declarations};

    pub(crate) fn module_type_implicit_global_declaration<'db>(
        db: &'db dyn Db,
        expr: &PlaceExpr,
    ) -> PlaceFromDeclarationsResult<'db> {
        if !module_type_symbols(db)
            .iter()
            .any(|module_type_member| Some(module_type_member) == expr.as_name())
        {
            return Ok(Place::Unbound.into());
        }
        let Type::ClassLiteral(module_type_class) = KnownClass::ModuleType.to_class_literal(db)
        else {
            return Ok(Place::Unbound.into());
        };
        let module_type_scope = module_type_class.body_scope(db);
        let place_table = place_table(db, module_type_scope);
        let Some(place_id) = place_table.place_id_by_expr(expr) else {
            return Ok(Place::Unbound.into());
        };
        place_from_declarations(
            db,
            use_def_map(db, module_type_scope).public_declarations(place_id),
        )
    }

    /// Looks up the type of an "implicit global symbol". Returns [`Place::Unbound`] if
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
    /// [`Place::Unbound`] for `__init__` and `__dict__` (which cannot be found in globals if
    /// the lookup is being done from the same file) -- but these symbols *are* available in the
    /// global scope if they're being imported **from a different file**.
    pub(crate) fn module_type_implicit_global_symbol<'db>(
        db: &'db dyn Db,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        // We special-case `__file__` here because we know that for an internal implicit global
        // lookup in a Python module, it is always a string, even though typeshed says `str |
        // None`.
        if name == "__file__" {
            Place::bound(KnownClass::Str.to_instance(db)).into()
        } else if name == "__builtins__" {
            Place::bound(Type::any()).into()
        } else if name == "__debug__" {
            Place::bound(KnownClass::Bool.to_instance(db)).into()
        }
        // In general we wouldn't check to see whether a symbol exists on a class before doing the
        // `.member()` call on the instance type -- we'd just do the `.member`() call on the instance
        // type, since it has the same end result. The reason to only call `.member()` on `ModuleType`
        // when absolutely necessary is that this function is used in a very hot path (name resolution
        // in `infer.rs`). We use less idiomatic (and much more verbose) code here as a micro-optimisation.
        else if module_type_symbols(db)
            .iter()
            .any(|module_type_member| &**module_type_member == name)
        {
            KnownClass::ModuleType.to_instance(db).member(db, name)
        } else {
            Place::Unbound.into()
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
    #[salsa::tracked(returns(deref))]
    fn module_type_symbols<'db>(db: &'db dyn Db) -> smallvec::SmallVec<[ast::name::Name; 8]> {
        let Some(module_type) = KnownClass::ModuleType
            .to_class_literal(db)
            .into_class_literal()
        else {
            // The most likely way we get here is if a user specified a `--custom-typeshed-dir`
            // without a `types.pyi` stub in the `stdlib/` directory
            return smallvec::SmallVec::default();
        };

        let module_type_scope = module_type.body_scope(db);
        let module_type_symbol_table = place_table(db, module_type_scope);

        module_type_symbol_table
            .places()
            .filter(|place| place.is_declared() && place.is_name())
            .map(semantic_index::place::PlaceExprWithFlags::expect_name)
            .filter(|symbol_name| {
                !matches!(&***symbol_name, "__dict__" | "__getattr__" | "__init__")
            })
            .cloned()
            .collect()
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

/// Computes a possibly-widened type `Unknown | T_inferred` from the inferred type `T_inferred`
/// of a symbol, unless the type is a known-instance type (e.g. `typing.Any`) or the symbol is
/// considered non-modifiable (e.g. when the symbol is `@Final`). We need this for public uses
/// of symbols that have no declared type.
fn widen_type_for_undeclared_public_symbol<'db>(
    db: &'db dyn Db,
    inferred: Place<'db>,
    is_considered_non_modifiable: bool,
) -> Place<'db> {
    // We special-case known-instance types here since symbols like `typing.Any` are typically
    // not declared in the stubs (e.g. `Any = object()`), but we still want to treat them as
    // such.
    let is_known_instance = inferred
        .ignore_possibly_unbound()
        .is_some_and(|ty| matches!(ty, Type::SpecialForm(_) | Type::KnownInstance(_)));

    if is_considered_non_modifiable || is_known_instance {
        inferred
    } else {
        inferred.map_type(|ty| UnionType::from_elements(db, [Type::unknown(), ty]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::setup_db;

    #[test]
    fn test_symbol_or_fall_back_to() {
        use Boundness::{Bound, PossiblyUnbound};

        let db = setup_db();
        let ty1 = Type::IntLiteral(1);
        let ty2 = Type::IntLiteral(2);

        let unbound = || Place::Unbound.with_qualifiers(TypeQualifiers::empty());

        let possibly_unbound_ty1 =
            || Place::Type(ty1, PossiblyUnbound).with_qualifiers(TypeQualifiers::empty());
        let possibly_unbound_ty2 =
            || Place::Type(ty2, PossiblyUnbound).with_qualifiers(TypeQualifiers::empty());

        let bound_ty1 = || Place::Type(ty1, Bound).with_qualifiers(TypeQualifiers::empty());
        let bound_ty2 = || Place::Type(ty2, Bound).with_qualifiers(TypeQualifiers::empty());

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
            Place::Type(UnionType::from_elements(&db, [ty1, ty2]), PossiblyUnbound).into()
        );
        assert_eq!(
            possibly_unbound_ty1().or_fall_back_to(&db, bound_ty2),
            Place::Type(UnionType::from_elements(&db, [ty1, ty2]), Bound).into()
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
            Place::Type(Type::NominalInstance(_), Boundness::Bound)
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
