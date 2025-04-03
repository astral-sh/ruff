use ruff_db::files::File;

use crate::module_resolver::file_to_module;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::symbol::{ScopeId, ScopedSymbolId};
use crate::semantic_index::{global_scope, use_def_map, DeclarationWithConstraint};
use crate::semantic_index::{
    symbol_table, BindingWithConstraints, BindingWithConstraintsIterator, DeclarationsIterator,
};
use crate::types::{
    binding_type, declaration_type, infer_narrowing_constraint, todo_type, IntersectionBuilder,
    KnownClass, Truthiness, Type, TypeAndQualifiers, TypeQualifiers, UnionBuilder, UnionType,
};
use crate::{resolve_module, Db, KnownModule, Module, Program};

pub(crate) use implicit_globals::module_type_implicit_global_symbol;

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

/// The result of a symbol lookup, which can either be a (possibly unbound) type
/// or a completely unbound symbol.
///
/// Consider this example:
/// ```py
/// bound = 1
///
/// if flag:
///     possibly_unbound = 2
/// ```
///
/// If we look up symbols in this scope, we would get the following results:
/// ```rs
/// bound:             Symbol::Type(Type::IntLiteral(1), Boundness::Bound),
/// possibly_unbound:  Symbol::Type(Type::IntLiteral(2), Boundness::PossiblyUnbound),
/// non_existent:      Symbol::Unbound,
/// ```
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub(crate) enum Symbol<'db> {
    Type(Type<'db>, Boundness),
    Unbound,
}

impl<'db> Symbol<'db> {
    /// Constructor that creates a `Symbol` with boundness [`Boundness::Bound`].
    pub(crate) fn bound(ty: impl Into<Type<'db>>) -> Self {
        Symbol::Type(ty.into(), Boundness::Bound)
    }

    /// Constructor that creates a [`Symbol`] with a [`crate::types::TodoType`] type
    /// and boundness [`Boundness::Bound`].
    #[allow(unused_variables)] // Only unused in release builds
    pub(crate) fn todo(message: &'static str) -> Self {
        Symbol::Type(todo_type!(message), Boundness::Bound)
    }

    pub(crate) fn is_unbound(&self) -> bool {
        matches!(self, Symbol::Unbound)
    }

    /// Returns the type of the symbol, ignoring possible unboundness.
    ///
    /// If the symbol is *definitely* unbound, this function will return `None`. Otherwise,
    /// if there is at least one control-flow path where the symbol is bound, return the type.
    pub(crate) fn ignore_possibly_unbound(&self) -> Option<Type<'db>> {
        match self {
            Symbol::Type(ty, _) => Some(*ty),
            Symbol::Unbound => None,
        }
    }

    #[cfg(test)]
    #[track_caller]
    pub(crate) fn expect_type(self) -> Type<'db> {
        self.ignore_possibly_unbound()
            .expect("Expected a (possibly unbound) type, not an unbound symbol")
    }

    #[must_use]
    pub(crate) fn map_type(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Symbol<'db> {
        match self {
            Symbol::Type(ty, boundness) => Symbol::Type(f(ty), boundness),
            Symbol::Unbound => Symbol::Unbound,
        }
    }

    #[must_use]
    pub(crate) fn with_qualifiers(self, qualifiers: TypeQualifiers) -> SymbolAndQualifiers<'db> {
        SymbolAndQualifiers {
            symbol: self,
            qualifiers,
        }
    }
}

impl<'db> From<LookupResult<'db>> for SymbolAndQualifiers<'db> {
    fn from(value: LookupResult<'db>) -> Self {
        match value {
            Ok(type_and_qualifiers) => {
                Symbol::Type(type_and_qualifiers.inner_type(), Boundness::Bound)
                    .with_qualifiers(type_and_qualifiers.qualifiers())
            }
            Err(LookupError::Unbound(qualifiers)) => Symbol::Unbound.with_qualifiers(qualifiers),
            Err(LookupError::PossiblyUnbound(type_and_qualifiers)) => {
                Symbol::Type(type_and_qualifiers.inner_type(), Boundness::PossiblyUnbound)
                    .with_qualifiers(type_and_qualifiers.qualifiers())
            }
        }
    }
}

/// Possible ways in which a symbol lookup can (possibly or definitely) fail.
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
        fallback: SymbolAndQualifiers<'db>,
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

/// A [`Result`] type in which the `Ok` variant represents a definitely bound symbol
/// and the `Err` variant represents a symbol that is either definitely or possibly unbound.
///
/// Note that this type is exactly isomorphic to [`Symbol`].
/// In the future, we could possibly consider removing `Symbol` and using this type everywhere instead.
pub(crate) type LookupResult<'db> = Result<TypeAndQualifiers<'db>, LookupError<'db>>;

/// Infer the public type of a symbol (its type as seen from outside its scope) in the given
/// `scope`.
pub(crate) fn symbol<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
) -> SymbolAndQualifiers<'db> {
    symbol_impl(db, scope, name, RequiresExplicitReExport::No)
}

/// Infer the public type of a class symbol (its type as seen from outside its scope) in the given
/// `scope`.
pub(crate) fn class_symbol<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
) -> SymbolAndQualifiers<'db> {
    symbol_table(db, scope)
        .symbol_id_by_name(name)
        .map(|symbol| {
            let symbol_and_quals = symbol_by_id(db, scope, symbol, RequiresExplicitReExport::No);

            if symbol_and_quals.is_class_var() {
                // For declared class vars we do not need to check if they have bindings,
                // we just trust the declaration.
                return symbol_and_quals;
            }

            if let SymbolAndQualifiers {
                symbol: Symbol::Type(ty, _),
                qualifiers,
            } = symbol_and_quals
            {
                // Otherwise, we need to check if the symbol has bindings
                let use_def = use_def_map(db, scope);
                let bindings = use_def.public_bindings(symbol);
                let inferred =
                    symbol_from_bindings_impl(db, bindings, RequiresExplicitReExport::No);

                // TODO: we should not need to calculate inferred type second time. This is a temporary
                // solution until the notion of Boundness and Declaredness is split. See #16036, #16264
                match inferred {
                    Symbol::Unbound => Symbol::Unbound.with_qualifiers(qualifiers),
                    Symbol::Type(_, boundness) => {
                        Symbol::Type(ty, boundness).with_qualifiers(qualifiers)
                    }
                }
            } else {
                Symbol::Unbound.into()
            }
        })
        .unwrap_or_default()
}

/// Infers the public type of an explicit module-global symbol as seen from within the same file.
///
/// Note that all global scopes also include various "implicit globals" such as `__name__`,
/// `__doc__` and `__file__`. This function **does not** consider those symbols; it will return
/// `Symbol::Unbound` for them. Use the (currently test-only) `global_symbol` query to also include
/// those additional symbols.
///
/// Use [`imported_symbol`] to perform the lookup as seen from outside the file (e.g. via imports).
pub(crate) fn explicit_global_symbol<'db>(
    db: &'db dyn Db,
    file: File,
    name: &str,
) -> SymbolAndQualifiers<'db> {
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
#[cfg(test)]
pub(crate) fn global_symbol<'db>(
    db: &'db dyn Db,
    file: File,
    name: &str,
) -> SymbolAndQualifiers<'db> {
    explicit_global_symbol(db, file, name)
        .or_fall_back_to(db, || module_type_implicit_global_symbol(db, name))
}

/// Infers the public type of an imported symbol.
pub(crate) fn imported_symbol<'db>(
    db: &'db dyn Db,
    module: &Module,
    name: &str,
) -> SymbolAndQualifiers<'db> {
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
    external_symbol_impl(db, module.file(), name).or_fall_back_to(db, || {
        if name == "__getattr__" {
            Symbol::Unbound.into()
        } else {
            KnownClass::ModuleType.to_instance(db).member(db, name)
        }
    })
}

/// Lookup the type of `symbol` in the builtins namespace.
///
/// Returns `Symbol::Unbound` if the `builtins` module isn't available for some reason.
///
/// Note that this function is only intended for use in the context of the builtins *namespace*
/// and should not be used when a symbol is being explicitly imported from the `builtins` module
/// (e.g. `from builtins import int`).
pub(crate) fn builtins_symbol<'db>(db: &'db dyn Db, symbol: &str) -> SymbolAndQualifiers<'db> {
    resolve_module(db, &KnownModule::Builtins.name())
        .map(|module| {
            external_symbol_impl(db, module.file(), symbol).or_fall_back_to(db, || {
                // We're looking up in the builtins namespace and not the module, so we should
                // do the normal lookup in `types.ModuleType` and not the special one as in
                // `imported_symbol`.
                module_type_implicit_global_symbol(db, symbol)
            })
        })
        .unwrap_or_default()
}

/// Lookup the type of `symbol` in a given known module.
///
/// Returns `Symbol::Unbound` if the given known module cannot be resolved for some reason.
pub(crate) fn known_module_symbol<'db>(
    db: &'db dyn Db,
    known_module: KnownModule,
    symbol: &str,
) -> SymbolAndQualifiers<'db> {
    resolve_module(db, &known_module.name())
        .map(|module| imported_symbol(db, &module, symbol))
        .unwrap_or_default()
}

/// Lookup the type of `symbol` in the `typing` module namespace.
///
/// Returns `Symbol::Unbound` if the `typing` module isn't available for some reason.
#[inline]
#[cfg(test)]
pub(crate) fn typing_symbol<'db>(db: &'db dyn Db, symbol: &str) -> SymbolAndQualifiers<'db> {
    known_module_symbol(db, KnownModule::Typing, symbol)
}

/// Lookup the type of `symbol` in the `typing_extensions` module namespace.
///
/// Returns `Symbol::Unbound` if the `typing_extensions` module isn't available for some reason.
#[inline]
pub(crate) fn typing_extensions_symbol<'db>(
    db: &'db dyn Db,
    symbol: &str,
) -> SymbolAndQualifiers<'db> {
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
    resolve_module(db, &core_module.name()).map(|module| global_scope(db, module.file()))
}

/// Infer the combined type from an iterator of bindings, and return it
/// together with boundness information in a [`Symbol`].
///
/// The type will be a union if there are multiple bindings with different types.
pub(super) fn symbol_from_bindings<'db>(
    db: &'db dyn Db,
    bindings_with_constraints: BindingWithConstraintsIterator<'_, 'db>,
) -> Symbol<'db> {
    symbol_from_bindings_impl(db, bindings_with_constraints, RequiresExplicitReExport::No)
}

/// Build a declared type from a [`DeclarationsIterator`].
///
/// If there is only one declaration, or all declarations declare the same type, returns
/// `Ok(..)`. If there are conflicting declarations, returns an `Err(..)` variant with
/// a union of the declared types as well as a list of all conflicting types.
///
/// This function also returns declaredness information (see [`Symbol`]) and a set of
/// [`TypeQualifiers`] that have been specified on the declaration(s).
pub(crate) fn symbol_from_declarations<'db>(
    db: &'db dyn Db,
    declarations: DeclarationsIterator<'_, 'db>,
) -> SymbolFromDeclarationsResult<'db> {
    symbol_from_declarations_impl(db, declarations, RequiresExplicitReExport::No)
}

/// The result of looking up a declared type from declarations; see [`symbol_from_declarations`].
pub(crate) type SymbolFromDeclarationsResult<'db> =
    Result<SymbolAndQualifiers<'db>, (TypeAndQualifiers<'db>, Box<[Type<'db>]>)>;

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
pub(crate) struct SymbolAndQualifiers<'db> {
    pub(crate) symbol: Symbol<'db>,
    pub(crate) qualifiers: TypeQualifiers,
}

impl Default for SymbolAndQualifiers<'_> {
    fn default() -> Self {
        SymbolAndQualifiers {
            symbol: Symbol::Unbound,
            qualifiers: TypeQualifiers::empty(),
        }
    }
}

impl<'db> SymbolAndQualifiers<'db> {
    /// Constructor that creates a [`SymbolAndQualifiers`] instance with a [`TodoType`] type
    /// and no qualifiers.
    ///
    /// [`TodoType`]: crate::types::TodoType
    pub(crate) fn todo(message: &'static str) -> Self {
        Self {
            symbol: Symbol::todo(message),
            qualifiers: TypeQualifiers::empty(),
        }
    }

    /// Returns `true` if the symbol has a `ClassVar` type qualifier.
    pub(crate) fn is_class_var(&self) -> bool {
        self.qualifiers.contains(TypeQualifiers::CLASS_VAR)
    }

    /// Transform symbol and qualifiers into a [`LookupResult`],
    /// a [`Result`] type in which the `Ok` variant represents a definitely bound symbol
    /// and the `Err` variant represents a symbol that is either definitely or possibly unbound.
    pub(crate) fn into_lookup_result(self) -> LookupResult<'db> {
        match self {
            SymbolAndQualifiers {
                symbol: Symbol::Type(ty, Boundness::Bound),
                qualifiers,
            } => Ok(TypeAndQualifiers::new(ty, qualifiers)),
            SymbolAndQualifiers {
                symbol: Symbol::Type(ty, Boundness::PossiblyUnbound),
                qualifiers,
            } => Err(LookupError::PossiblyUnbound(TypeAndQualifiers::new(
                ty, qualifiers,
            ))),
            SymbolAndQualifiers {
                symbol: Symbol::Unbound,
                qualifiers,
            } => Err(LookupError::Unbound(qualifiers)),
        }
    }

    /// Safely unwrap the symbol and the qualifiers into a [`TypeQualifiers`].
    ///
    /// If the symbol is definitely unbound or possibly unbound, it will be transformed into a
    /// [`LookupError`] and `diagnostic_fn` will be applied to the error value before returning
    /// the result of `diagnostic_fn` (which will be a [`TypeQualifiers`]). This allows the caller
    /// to ensure that a diagnostic is emitted if the symbol is possibly or definitely unbound.
    pub(crate) fn unwrap_with_diagnostic(
        self,
        diagnostic_fn: impl FnOnce(LookupError<'db>) -> TypeAndQualifiers<'db>,
    ) -> TypeAndQualifiers<'db> {
        self.into_lookup_result().unwrap_or_else(diagnostic_fn)
    }

    /// Fallback (partially or fully) to another symbol if `self` is partially or fully unbound.
    ///
    /// 1. If `self` is definitely bound, return `self` without evaluating `fallback_fn()`.
    /// 2. Else, evaluate `fallback_fn()`:
    ///    1. If `self` is definitely unbound, return the result of `fallback_fn()`.
    ///    2. Else, if `fallback` is definitely unbound, return `self`.
    ///    3. Else, if `self` is possibly unbound and `fallback` is definitely bound,
    ///       return `Symbol(<union of self-type and fallback-type>, Boundness::Bound)`
    ///    4. Else, if `self` is possibly unbound and `fallback` is possibly unbound,
    ///       return `Symbol(<union of self-type and fallback-type>, Boundness::PossiblyUnbound)`
    #[must_use]
    pub(crate) fn or_fall_back_to(
        self,
        db: &'db dyn Db,
        fallback_fn: impl FnOnce() -> SymbolAndQualifiers<'db>,
    ) -> Self {
        self.into_lookup_result()
            .or_else(|lookup_error| lookup_error.or_fall_back_to(db, fallback_fn()))
            .into()
    }
}

impl<'db> From<Symbol<'db>> for SymbolAndQualifiers<'db> {
    fn from(symbol: Symbol<'db>) -> Self {
        symbol.with_qualifiers(TypeQualifiers::empty())
    }
}

fn symbol_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &SymbolAndQualifiers<'db>,
    _count: u32,
    _scope: ScopeId<'db>,
    _symbol_id: ScopedSymbolId,
    _requires_explicit_reexport: RequiresExplicitReExport,
) -> salsa::CycleRecoveryAction<SymbolAndQualifiers<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn symbol_cycle_initial<'db>(
    _db: &'db dyn Db,
    _scope: ScopeId<'db>,
    _symbol_id: ScopedSymbolId,
    _requires_explicit_reexport: RequiresExplicitReExport,
) -> SymbolAndQualifiers<'db> {
    Symbol::bound(Type::Never).into()
}

#[salsa::tracked(cycle_fn=symbol_cycle_recover, cycle_initial=symbol_cycle_initial)]
fn symbol_by_id<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol_id: ScopedSymbolId,
    requires_explicit_reexport: RequiresExplicitReExport,
) -> SymbolAndQualifiers<'db> {
    let use_def = use_def_map(db, scope);

    // If the symbol is declared, the public type is based on declarations; otherwise, it's based
    // on inference from bindings.

    let declarations = use_def.public_declarations(symbol_id);
    let declared = symbol_from_declarations_impl(db, declarations, requires_explicit_reexport);

    match declared {
        // Symbol is declared, trust the declared type
        Ok(
            symbol_and_quals @ SymbolAndQualifiers {
                symbol: Symbol::Type(_, Boundness::Bound),
                qualifiers: _,
            },
        ) => symbol_and_quals,
        // Symbol is possibly declared
        Ok(SymbolAndQualifiers {
            symbol: Symbol::Type(declared_ty, Boundness::PossiblyUnbound),
            qualifiers,
        }) => {
            let bindings = use_def.public_bindings(symbol_id);
            let inferred = symbol_from_bindings_impl(db, bindings, requires_explicit_reexport);

            let symbol = match inferred {
                // Symbol is possibly undeclared and definitely unbound
                Symbol::Unbound => {
                    // TODO: We probably don't want to report `Bound` here. This requires a bit of
                    // design work though as we might want a different behavior for stubs and for
                    // normal modules.
                    Symbol::Type(declared_ty, Boundness::Bound)
                }
                // Symbol is possibly undeclared and (possibly) bound
                Symbol::Type(inferred_ty, boundness) => Symbol::Type(
                    UnionType::from_elements(db, [inferred_ty, declared_ty]),
                    boundness,
                ),
            };

            SymbolAndQualifiers { symbol, qualifiers }
        }
        // Symbol is undeclared, return the union of `Unknown` with the inferred type
        Ok(SymbolAndQualifiers {
            symbol: Symbol::Unbound,
            qualifiers: _,
        }) => {
            let bindings = use_def.public_bindings(symbol_id);
            let inferred = symbol_from_bindings_impl(db, bindings, requires_explicit_reexport);

            // `__slots__` is a symbol with special behavior in Python's runtime. It can be
            // modified externally, but those changes do not take effect. We therefore issue
            // a diagnostic if we see it being modified externally. In type inference, we
            // can assign a "narrow" type to it even if it is not *declared*. This means, we
            // do not have to call [`widen_type_for_undeclared_public_symbol`].
            //
            // `TYPE_CHECKING` is a special variable that should only be assigned `False`
            // at runtime, but is always considered `True` in type checking.
            // See mdtest/known_constants.md#user-defined-type_checking for details.
            let is_considered_non_modifiable = matches!(
                symbol_table(db, scope).symbol(symbol_id).name().as_str(),
                "__slots__" | "TYPE_CHECKING"
            );

            widen_type_for_undeclared_public_symbol(db, inferred, is_considered_non_modifiable)
                .into()
        }
        // Symbol has conflicting declared types
        Err((declared, _)) => {
            // Intentionally ignore conflicting declared types; that's not our problem,
            // it's the problem of the module we are importing from.
            Symbol::bound(declared.inner_type()).with_qualifiers(declared.qualifiers())
        }
    }

    // TODO (ticket: https://github.com/astral-sh/ruff/issues/14297) Our handling of boundness
    // currently only depends on bindings, and ignores declarations. This is inconsistent, since
    // we only look at bindings if the symbol may be undeclared. Consider the following example:
    // ```py
    // x: int
    //
    // if flag:
    //     y: int
    // else
    //     y = 3
    // ```
    // If we import from this module, we will currently report `x` as a definitely-bound symbol
    // (even though it has no bindings at all!) but report `y` as possibly-unbound (even though
    // every path has either a binding or a declaration for it.)
}

/// Implementation of [`symbol`].
fn symbol_impl<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
    requires_explicit_reexport: RequiresExplicitReExport,
) -> SymbolAndQualifiers<'db> {
    let _span = tracing::trace_span!("symbol", ?name).entered();

    if name == "platform"
        && file_to_module(db, scope.file(db))
            .is_some_and(|module| module.is_known(KnownModule::Sys))
    {
        match Program::get(db).python_platform(db) {
            crate::PythonPlatform::Identifier(platform) => {
                return Symbol::bound(Type::string_literal(db, platform.as_str())).into();
            }
            crate::PythonPlatform::All => {
                // Fall through to the looked up type
            }
        }
    }

    symbol_table(db, scope)
        .symbol_id_by_name(name)
        .map(|symbol| symbol_by_id(db, scope, symbol, requires_explicit_reexport))
        .unwrap_or_default()
}

/// Implementation of [`symbol_from_bindings`].
///
/// ## Implementation Note
/// This function gets called cross-module. It, therefore, shouldn't
/// access any AST nodes from the file containing the declarations.
fn symbol_from_bindings_impl<'db>(
    db: &'db dyn Db,
    bindings_with_constraints: BindingWithConstraintsIterator<'_, 'db>,
    requires_explicit_reexport: RequiresExplicitReExport,
) -> Symbol<'db> {
    let predicates = bindings_with_constraints.predicates;
    let visibility_constraints = bindings_with_constraints.visibility_constraints;
    let mut bindings_with_constraints = bindings_with_constraints.peekable();

    let is_non_exported = |binding: Definition<'db>| {
        requires_explicit_reexport.is_yes() && !binding.is_reexported(db)
    };

    let unbound_visibility = match bindings_with_constraints.peek() {
        Some(BindingWithConstraints {
            binding,
            visibility_constraint,
            narrowing_constraint: _,
        }) if binding.is_none_or(is_non_exported) => {
            visibility_constraints.evaluate(db, predicates, *visibility_constraint)
        }
        _ => Truthiness::AlwaysFalse,
    };

    let mut types = bindings_with_constraints.filter_map(
        |BindingWithConstraints {
             binding,
             narrowing_constraint,
             visibility_constraint,
         }| {
            let binding = binding?;

            if is_non_exported(binding) {
                return None;
            }

            let static_visibility =
                visibility_constraints.evaluate(db, predicates, visibility_constraint);

            if static_visibility.is_always_false() {
                // We found a binding that we have statically determined to not be visible from
                // the use of the symbol that we are investigating. There are three interesting
                // cases to consider:
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
                //     z = 1
                //     if flag:
                //         z = 2
                //         return
                //     use(z)
                // ```
                //
                // In the first case, there is a single binding for `x`, and due to the statically
                // known `False` condition, it is not visible at the use of `x`. However, we *can*
                // see/reach the start of the scope from `use(x)`. This means that `x` is unbound
                // and we should return `None`.
                //
                // In the second case, `y` is also not visible at the use of `y`, but here, we can
                // not see/reach the start of the scope. There is only one path of control flow,
                // and it passes through that binding of `y` (which we can not see). This implies
                // that we are in an unreachable section of code. We return `Never` in order to
                // silence the `unresolve-reference` diagnostic that would otherwise be emitted at
                // the use of `y`.
                //
                // In the third case, we have two bindings for `z`. The first one is visible, so we
                // consider the case that we now encounter the second binding `z = 2`, which is not
                // visible due to the early return. We *also* can not see the start of the scope
                // from `use(z)` because both paths of control flow pass through a binding of `z`.
                // The `z = 1` binding is visible, and so we are *not* in an unreachable section of
                // code. However, it is still okay to return `Never` in this case, because we will
                // union the types of all bindings, and `Never` will be eliminated automatically.

                if unbound_visibility.is_always_false() {
                    // The scope-start is not visible
                    return Some(Type::Never);
                }
                return None;
            }

            let constraint_tys: Vec<_> = narrowing_constraint
                .filter_map(|constraint| infer_narrowing_constraint(db, constraint, binding))
                .collect();

            let binding_ty = binding_type(db, binding);
            if constraint_tys.is_empty() {
                Some(binding_ty)
            } else {
                let intersection_ty = constraint_tys
                    .into_iter()
                    .rev()
                    .fold(
                        IntersectionBuilder::new(db).add_positive(binding_ty),
                        IntersectionBuilder::add_positive,
                    )
                    .build();
                Some(intersection_ty)
            }
        },
    );

    if let Some(first) = types.next() {
        let boundness = match unbound_visibility {
            Truthiness::AlwaysTrue => {
                unreachable!("If we have at least one binding, the scope-start should not be definitely visible")
            }
            Truthiness::AlwaysFalse => Boundness::Bound,
            Truthiness::Ambiguous => Boundness::PossiblyUnbound,
        };

        if let Some(second) = types.next() {
            Symbol::Type(
                UnionType::from_elements(db, [first, second].into_iter().chain(types)),
                boundness,
            )
        } else {
            Symbol::Type(first, boundness)
        }
    } else {
        Symbol::Unbound
    }
}

/// Implementation of [`symbol_from_declarations`].
///
/// ## Implementation Note
/// This function gets called cross-module. It, therefore, shouldn't
/// access any AST nodes from the file containing the declarations.
fn symbol_from_declarations_impl<'db>(
    db: &'db dyn Db,
    declarations: DeclarationsIterator<'_, 'db>,
    requires_explicit_reexport: RequiresExplicitReExport,
) -> SymbolFromDeclarationsResult<'db> {
    let predicates = declarations.predicates;
    let visibility_constraints = declarations.visibility_constraints;
    let mut declarations = declarations.peekable();

    let is_non_exported = |declaration: Definition<'db>| {
        requires_explicit_reexport.is_yes() && !declaration.is_reexported(db)
    };

    let undeclared_visibility = match declarations.peek() {
        Some(DeclarationWithConstraint {
            declaration,
            visibility_constraint,
        }) if declaration.is_none_or(is_non_exported) => {
            visibility_constraints.evaluate(db, predicates, *visibility_constraint)
        }
        _ => Truthiness::AlwaysFalse,
    };

    let mut types = declarations.filter_map(
        |DeclarationWithConstraint {
             declaration,
             visibility_constraint,
         }| {
            let declaration = declaration?;

            if is_non_exported(declaration) {
                return None;
            }

            let static_visibility =
                visibility_constraints.evaluate(db, predicates, visibility_constraint);

            if static_visibility.is_always_false() {
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
            let boundness = match undeclared_visibility {
                Truthiness::AlwaysTrue => {
                    unreachable!("If we have at least one declaration, the scope-start should not be definitely visible")
                }
                Truthiness::AlwaysFalse => Boundness::Bound,
                Truthiness::Ambiguous => Boundness::PossiblyUnbound,
            };

            Ok(Symbol::Type(declared.inner_type(), boundness)
                .with_qualifiers(declared.qualifiers()))
        } else {
            Err((
                declared,
                std::iter::once(first.inner_type())
                    .chain(conflicting)
                    .collect(),
            ))
        }
    } else {
        Ok(Symbol::Unbound.into())
    }
}

mod implicit_globals {
    use ruff_python_ast as ast;

    use crate::db::Db;
    use crate::semantic_index::{self, symbol_table};
    use crate::symbol::SymbolAndQualifiers;
    use crate::types::KnownClass;

    use super::Symbol;

    /// Looks up the type of an "implicit global symbol". Returns [`Symbol::Unbound`] if
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
    /// [`Symbol::Unbound`] for `__init__` and `__dict__` (which cannot be found in globals if
    /// the lookup is being done from the same file) -- but these symbols *are* available in the
    /// global scope if they're being imported **from a different file**.
    pub(crate) fn module_type_implicit_global_symbol<'db>(
        db: &'db dyn Db,
        name: &str,
    ) -> SymbolAndQualifiers<'db> {
        // In general we wouldn't check to see whether a symbol exists on a class before doing the
        // `.member()` call on the instance type -- we'd just do the `.member`() call on the instance
        // type, since it has the same end result. The reason to only call `.member()` on `ModuleType`
        // when absolutely necessary is that this function is used in a very hot path (name resolution
        // in `infer.rs`). We use less idiomatic (and much more verbose) code here as a micro-optimisation.
        if module_type_symbols(db)
            .iter()
            .any(|module_type_member| &**module_type_member == name)
        {
            KnownClass::ModuleType.to_instance(db).member(db, name)
        } else {
            Symbol::Unbound.into()
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
    #[salsa::tracked(return_ref)]
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
        let module_type_symbol_table = symbol_table(db, module_type_scope);

        module_type_symbol_table
            .symbols()
            .filter(|symbol| symbol.is_declared())
            .map(semantic_index::symbol::Symbol::name)
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

/// Implementation of looking up a module-global symbol as seen from outside the file (e.g. via
/// imports).
///
/// This will take into account whether the definition of the symbol is being explicitly
/// re-exported from a stub file or not.
fn external_symbol_impl<'db>(db: &'db dyn Db, file: File, name: &str) -> SymbolAndQualifiers<'db> {
    symbol_impl(
        db,
        global_scope(db, file),
        name,
        if file.is_stub(db.upcast()) {
            RequiresExplicitReExport::Yes
        } else {
            RequiresExplicitReExport::No
        },
    )
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum RequiresExplicitReExport {
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
    inferred: Symbol<'db>,
    is_considered_non_modifiable: bool,
) -> Symbol<'db> {
    // We special-case known-instance types here since symbols like `typing.Any` are typically
    // not declared in the stubs (e.g. `Any = object()`), but we still want to treat them as
    // such.
    let is_known_instance = inferred
        .ignore_possibly_unbound()
        .is_some_and(|ty| matches!(ty, Type::KnownInstance(_)));

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

        let unbound = || Symbol::Unbound.with_qualifiers(TypeQualifiers::empty());

        let possibly_unbound_ty1 =
            || Symbol::Type(ty1, PossiblyUnbound).with_qualifiers(TypeQualifiers::empty());
        let possibly_unbound_ty2 =
            || Symbol::Type(ty2, PossiblyUnbound).with_qualifiers(TypeQualifiers::empty());

        let bound_ty1 = || Symbol::Type(ty1, Bound).with_qualifiers(TypeQualifiers::empty());
        let bound_ty2 = || Symbol::Type(ty2, Bound).with_qualifiers(TypeQualifiers::empty());

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
            Symbol::Type(UnionType::from_elements(&db, [ty1, ty2]), PossiblyUnbound).into()
        );
        assert_eq!(
            possibly_unbound_ty1().or_fall_back_to(&db, bound_ty2),
            Symbol::Type(UnionType::from_elements(&db, [ty1, ty2]), Bound).into()
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
    fn assert_bound_string_symbol<'db>(db: &'db dyn Db, symbol: Symbol<'db>) {
        assert!(matches!(
            symbol,
            Symbol::Type(Type::Instance(_), Boundness::Bound)
        ));
        assert_eq!(symbol.expect_type(), KnownClass::Str.to_instance(db));
    }

    #[test]
    fn implicit_builtin_globals() {
        let db = setup_db();
        assert_bound_string_symbol(&db, builtins_symbol(&db, "__name__").symbol);
    }

    #[test]
    fn implicit_typing_globals() {
        let db = setup_db();
        assert_bound_string_symbol(&db, typing_symbol(&db, "__name__").symbol);
    }

    #[test]
    fn implicit_typing_extensions_globals() {
        let db = setup_db();
        assert_bound_string_symbol(&db, typing_extensions_symbol(&db, "__name__").symbol);
    }

    #[test]
    fn implicit_sys_globals() {
        let db = setup_db();
        assert_bound_string_symbol(
            &db,
            known_module_symbol(&db, KnownModule::Sys, "__name__").symbol,
        );
    }
}
