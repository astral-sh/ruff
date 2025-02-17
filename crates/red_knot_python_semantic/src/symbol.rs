use ruff_db::files::File;
use ruff_python_ast as ast;

use crate::module_resolver::file_to_module;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::symbol::{ScopeId, ScopedSymbolId};
use crate::semantic_index::{self, global_scope, use_def_map, DeclarationWithConstraint};
use crate::semantic_index::{
    symbol_table, BindingWithConstraints, BindingWithConstraintsIterator, DeclarationsIterator,
};
use crate::types::{
    binding_type, declaration_type, narrowing_constraint, todo_type, IntersectionBuilder,
    KnownClass, Truthiness, Type, TypeAndQualifiers, TypeQualifiers, UnionBuilder, UnionType,
};
use crate::{resolve_module, Db, KnownModule, Module, Program};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Boundness {
    Bound,
    PossiblyUnbound,
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

    /// Transform the symbol into a [`LookupResult`],
    /// a [`Result`] type in which the `Ok` variant represents a definitely bound symbol
    /// and the `Err` variant represents a symbol that is either definitely or possibly unbound.
    pub(crate) fn into_lookup_result(self) -> LookupResult<'db> {
        match self {
            Symbol::Type(ty, Boundness::Bound) => Ok(ty),
            Symbol::Type(ty, Boundness::PossiblyUnbound) => Err(LookupError::PossiblyUnbound(ty)),
            Symbol::Unbound => Err(LookupError::Unbound),
        }
    }

    /// Safely unwrap the symbol into a [`Type`].
    ///
    /// If the symbol is definitely unbound or possibly unbound, it will be transformed into a
    /// [`LookupError`] and `diagnostic_fn` will be applied to the error value before returning
    /// the result of `diagnostic_fn` (which will be a [`Type`]). This allows the caller to ensure
    /// that a diagnostic is emitted if the symbol is possibly or definitely unbound.
    pub(crate) fn unwrap_with_diagnostic(
        self,
        diagnostic_fn: impl FnOnce(LookupError<'db>) -> Type<'db>,
    ) -> Type<'db> {
        self.into_lookup_result().unwrap_or_else(diagnostic_fn)
    }

    /// Fallback (partially or fully) to another symbol if `self` is partially or fully unbound.
    ///
    /// 1. If `self` is definitely bound, return `self` without evaluating `fallback_fn()`.
    /// 2. Else, evaluate `fallback_fn()`:
    ///    a. If `self` is definitely unbound, return the result of `fallback_fn()`.
    ///    b. Else, if `fallback` is definitely unbound, return `self`.
    ///    c. Else, if `self` is possibly unbound and `fallback` is definitely bound,
    ///       return `Symbol(<union of self-type and fallback-type>, Boundness::Bound)`
    ///    d. Else, if `self` is possibly unbound and `fallback` is possibly unbound,
    ///       return `Symbol(<union of self-type and fallback-type>, Boundness::PossiblyUnbound)`
    #[must_use]
    pub(crate) fn or_fall_back_to(
        self,
        db: &'db dyn Db,
        fallback_fn: impl FnOnce() -> Self,
    ) -> Self {
        self.into_lookup_result()
            .or_else(|lookup_error| lookup_error.or_fall_back_to(db, fallback_fn()))
            .into()
    }

    #[must_use]
    pub(crate) fn map_type(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Symbol<'db> {
        match self {
            Symbol::Type(ty, boundness) => Symbol::Type(f(ty), boundness),
            Symbol::Unbound => Symbol::Unbound,
        }
    }
}

impl<'db> From<LookupResult<'db>> for Symbol<'db> {
    fn from(value: LookupResult<'db>) -> Self {
        match value {
            Ok(ty) => Symbol::Type(ty, Boundness::Bound),
            Err(LookupError::Unbound) => Symbol::Unbound,
            Err(LookupError::PossiblyUnbound(ty)) => Symbol::Type(ty, Boundness::PossiblyUnbound),
        }
    }
}

/// Possible ways in which a symbol lookup can (possibly or definitely) fail.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum LookupError<'db> {
    Unbound,
    PossiblyUnbound(Type<'db>),
}

impl<'db> LookupError<'db> {
    /// Fallback (wholly or partially) to `fallback` to create a new [`LookupResult`].
    pub(crate) fn or_fall_back_to(
        self,
        db: &'db dyn Db,
        fallback: Symbol<'db>,
    ) -> LookupResult<'db> {
        let fallback = fallback.into_lookup_result();
        match (&self, &fallback) {
            (LookupError::Unbound, _) => fallback,
            (LookupError::PossiblyUnbound { .. }, Err(LookupError::Unbound)) => Err(self),
            (LookupError::PossiblyUnbound(ty), Ok(ty2)) => {
                Ok(UnionType::from_elements(db, [ty, ty2]))
            }
            (LookupError::PossiblyUnbound(ty), Err(LookupError::PossiblyUnbound(ty2))) => Err(
                LookupError::PossiblyUnbound(UnionType::from_elements(db, [ty, ty2])),
            ),
        }
    }
}

/// A [`Result`] type in which the `Ok` variant represents a definitely bound symbol
/// and the `Err` variant represents a symbol that is either definitely or possibly unbound.
///
/// Note that this type is exactly isomorphic to [`Symbol`].
/// In the future, we could possibly consider removing `Symbol` and using this type everywhere instead.
pub(crate) type LookupResult<'db> = Result<Type<'db>, LookupError<'db>>;

/// Infer the public type of a symbol (its type as seen from outside its scope) in the given
/// `scope`.
pub(crate) fn symbol<'db>(db: &'db dyn Db, scope: ScopeId<'db>, name: &str) -> Symbol<'db> {
    symbol_impl(db, scope, name, RequiresExplicitReExport::No)
}

/// Infers the public type of a module-global symbol as seen from within the same file.
///
/// If it's not defined explicitly in the global scope, it will look it up in `types.ModuleType`
/// with a few very special exceptions.
///
/// Use [`imported_symbol`] to perform the lookup as seen from outside the file (e.g. via imports).
pub(crate) fn global_symbol<'db>(db: &'db dyn Db, file: File, name: &str) -> Symbol<'db> {
    symbol_impl(
        db,
        global_scope(db, file),
        name,
        RequiresExplicitReExport::No,
    )
    .or_fall_back_to(db, || module_type_symbol(db, name))
}

/// Infers the public type of an imported symbol.
pub(crate) fn imported_symbol<'db>(db: &'db dyn Db, module: &Module, name: &str) -> Symbol<'db> {
    // If it's not found in the global scope, check if it's present as an instance on
    // `types.ModuleType` or `builtins.object`.
    //
    // We do a more limited version of this in `global_symbol`, but there are two crucial
    // differences here:
    // - If a member is looked up as an attribute, `__init__` is also available on the module, but
    //   it isn't available as a global from inside the module
    // - If a member is looked up as an attribute, members on `builtins.object` are also available
    //   (because `types.ModuleType` inherits from `object`); these attributes are also not
    //   available as globals from inside the module.
    //
    // The same way as in `global_symbol`, however, we need to be careful to ignore
    // `__getattr__`. Typeshed has a fake `__getattr__` on `types.ModuleType` to help out with
    // dynamic imports; we shouldn't use it for `ModuleLiteral` types where we know exactly which
    // module we're dealing with.
    external_symbol_impl(db, module.file(), name).or_fall_back_to(db, || {
        if name == "__getattr__" {
            Symbol::Unbound
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
pub(crate) fn builtins_symbol<'db>(db: &'db dyn Db, symbol: &str) -> Symbol<'db> {
    resolve_module(db, &KnownModule::Builtins.name())
        .map(|module| {
            external_symbol_impl(db, module.file(), symbol).or_fall_back_to(db, || {
                // We're looking up in the builtins namespace and not the module, so we should
                // do the normal lookup in `types.ModuleType` and not the special one as in
                // `imported_symbol`.
                module_type_symbol(db, symbol)
            })
        })
        .unwrap_or(Symbol::Unbound)
}

/// Lookup the type of `symbol` in a given known module.
///
/// Returns `Symbol::Unbound` if the given known module cannot be resolved for some reason.
pub(crate) fn known_module_symbol<'db>(
    db: &'db dyn Db,
    known_module: KnownModule,
    symbol: &str,
) -> Symbol<'db> {
    resolve_module(db, &known_module.name())
        .map(|module| imported_symbol(db, &module, symbol))
        .unwrap_or(Symbol::Unbound)
}

/// Lookup the type of `symbol` in the `typing` module namespace.
///
/// Returns `Symbol::Unbound` if the `typing` module isn't available for some reason.
#[inline]
#[cfg(test)]
pub(crate) fn typing_symbol<'db>(db: &'db dyn Db, symbol: &str) -> Symbol<'db> {
    known_module_symbol(db, KnownModule::Typing, symbol)
}

/// Lookup the type of `symbol` in the `typing_extensions` module namespace.
///
/// Returns `Symbol::Unbound` if the `typing_extensions` module isn't available for some reason.
#[inline]
pub(crate) fn typing_extensions_symbol<'db>(db: &'db dyn Db, symbol: &str) -> Symbol<'db> {
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
pub(crate) fn symbol_from_bindings<'db>(
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
#[derive(Debug)]
pub(crate) struct SymbolAndQualifiers<'db>(pub(crate) Symbol<'db>, pub(crate) TypeQualifiers);

impl SymbolAndQualifiers<'_> {
    /// Constructor that creates a [`SymbolAndQualifiers`] instance with a [`TodoType`] type
    /// and no qualifiers.
    ///
    /// [`TodoType`]: crate::types::TodoType
    pub(crate) fn todo(message: &'static str) -> Self {
        Self(Symbol::todo(message), TypeQualifiers::empty())
    }

    /// Returns `true` if the symbol has a `ClassVar` type qualifier.
    pub(crate) fn is_class_var(&self) -> bool {
        self.1.contains(TypeQualifiers::CLASS_VAR)
    }

    /// Returns `true` if the symbol has a `Final` type qualifier.
    pub(crate) fn is_final(&self) -> bool {
        self.1.contains(TypeQualifiers::FINAL)
    }
}

impl<'db> From<Symbol<'db>> for SymbolAndQualifiers<'db> {
    fn from(symbol: Symbol<'db>) -> Self {
        SymbolAndQualifiers(symbol, TypeQualifiers::empty())
    }
}

/// Implementation of [`symbol`].
fn symbol_impl<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
    requires_explicit_reexport: RequiresExplicitReExport,
) -> Symbol<'db> {
    #[salsa::tracked]
    fn symbol_by_id<'db>(
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        symbol_id: ScopedSymbolId,
        requires_explicit_reexport: RequiresExplicitReExport,
    ) -> Symbol<'db> {
        let use_def = use_def_map(db, scope);

        // If the symbol is declared, the public type is based on declarations; otherwise, it's based
        // on inference from bindings.

        let declarations = use_def.public_declarations(symbol_id);
        let declared = symbol_from_declarations_impl(db, declarations, requires_explicit_reexport);
        let is_final = declared.as_ref().is_ok_and(SymbolAndQualifiers::is_final);
        let declared = declared.map(|SymbolAndQualifiers(symbol, _)| symbol);

        match declared {
            // Symbol is declared, trust the declared type
            Ok(symbol @ Symbol::Type(_, Boundness::Bound)) => symbol,
            // Symbol is possibly declared
            Ok(Symbol::Type(declared_ty, Boundness::PossiblyUnbound)) => {
                let bindings = use_def.public_bindings(symbol_id);
                let inferred = symbol_from_bindings_impl(db, bindings, requires_explicit_reexport);

                match inferred {
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
                }
            }
            // Symbol is undeclared, return the union of `Unknown` with the inferred type
            Ok(Symbol::Unbound) => {
                let bindings = use_def.public_bindings(symbol_id);
                let inferred = symbol_from_bindings_impl(db, bindings, requires_explicit_reexport);

                // `__slots__` is a symbol with special behavior in Python's runtime. It can be
                // modified externally, but those changes do not take effect. We therefore issue
                // a diagnostic if we see it being modified externally. In type inference, we
                // can assign a "narrow" type to it even if it is not *declared*. This means, we
                // do not have to call [`widen_type_for_undeclared_public_symbol`].
                let is_considered_non_modifiable =
                    is_final || symbol_table(db, scope).symbol(symbol_id).name() == "__slots__";

                widen_type_for_undeclared_public_symbol(db, inferred, is_considered_non_modifiable)
            }
            // Symbol has conflicting declared types
            Err((declared_ty, _)) => {
                // Intentionally ignore conflicting declared types; that's not our problem,
                // it's the problem of the module we are importing from.
                Symbol::bound(declared_ty.inner_type())
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

    let _span = tracing::trace_span!("symbol", ?name).entered();

    // We don't need to check for `typing_extensions` here, because `typing_extensions.TYPE_CHECKING`
    // is just a re-export of `typing.TYPE_CHECKING`.
    if name == "TYPE_CHECKING"
        && file_to_module(db, scope.file(db))
            .is_some_and(|module| module.is_known(KnownModule::Typing))
    {
        return Symbol::bound(Type::BooleanLiteral(true));
    }
    if name == "platform"
        && file_to_module(db, scope.file(db))
            .is_some_and(|module| module.is_known(KnownModule::Sys))
    {
        match Program::get(db).python_platform(db) {
            crate::PythonPlatform::Identifier(platform) => {
                return Symbol::bound(Type::string_literal(db, platform.as_str()));
            }
            crate::PythonPlatform::All => {
                // Fall through to the looked up type
            }
        }
    }

    symbol_table(db, scope)
        .symbol_id_by_name(name)
        .map(|symbol| symbol_by_id(db, scope, symbol, requires_explicit_reexport))
        .unwrap_or(Symbol::Unbound)
}

/// Implementation of [`symbol_from_bindings`].
fn symbol_from_bindings_impl<'db>(
    db: &'db dyn Db,
    bindings_with_constraints: BindingWithConstraintsIterator<'_, 'db>,
    requires_explicit_reexport: RequiresExplicitReExport,
) -> Symbol<'db> {
    let visibility_constraints = bindings_with_constraints.visibility_constraints;
    let mut bindings_with_constraints = bindings_with_constraints.peekable();

    let is_non_exported = |binding: Definition<'db>| {
        requires_explicit_reexport.is_yes() && !binding.is_reexported(db)
    };

    let unbound_visibility = match bindings_with_constraints.peek() {
        Some(BindingWithConstraints {
            binding,
            visibility_constraint,
            constraints: _,
        }) if binding.map_or(true, is_non_exported) => {
            visibility_constraints.evaluate(db, *visibility_constraint)
        }
        _ => Truthiness::AlwaysFalse,
    };

    let mut types = bindings_with_constraints.filter_map(
        |BindingWithConstraints {
             binding,
             constraints,
             visibility_constraint,
         }| {
            let binding = binding?;

            if is_non_exported(binding) {
                return None;
            }

            let static_visibility = visibility_constraints.evaluate(db, visibility_constraint);

            if static_visibility.is_always_false() {
                return None;
            }

            let mut constraint_tys = constraints
                .filter_map(|constraint| narrowing_constraint(db, constraint, binding))
                .peekable();

            let binding_ty = binding_type(db, binding);
            if constraint_tys.peek().is_some() {
                let intersection_ty = constraint_tys
                    .fold(
                        IntersectionBuilder::new(db).add_positive(binding_ty),
                        IntersectionBuilder::add_positive,
                    )
                    .build();
                Some(intersection_ty)
            } else {
                Some(binding_ty)
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
fn symbol_from_declarations_impl<'db>(
    db: &'db dyn Db,
    declarations: DeclarationsIterator<'_, 'db>,
    requires_explicit_reexport: RequiresExplicitReExport,
) -> SymbolFromDeclarationsResult<'db> {
    let visibility_constraints = declarations.visibility_constraints;
    let mut declarations = declarations.peekable();

    let is_non_exported = |declaration: Definition<'db>| {
        requires_explicit_reexport.is_yes() && !declaration.is_reexported(db)
    };

    let undeclared_visibility = match declarations.peek() {
        Some(DeclarationWithConstraint {
            declaration,
            visibility_constraint,
        }) if declaration.map_or(true, is_non_exported) => {
            visibility_constraints.evaluate(db, *visibility_constraint)
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

            let static_visibility = visibility_constraints.evaluate(db, visibility_constraint);

            if static_visibility.is_always_false() {
                None
            } else {
                Some(declaration_type(db, declaration))
            }
        },
    );

    if let Some(first) = types.next() {
        let mut conflicting: Vec<Type<'db>> = vec![];
        let declared_ty = if let Some(second) = types.next() {
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

            Ok(SymbolAndQualifiers(
                Symbol::Type(declared_ty.inner_type(), boundness),
                declared_ty.qualifiers(),
            ))
        } else {
            Err((
                declared_ty,
                std::iter::once(first.inner_type())
                    .chain(conflicting)
                    .collect(),
            ))
        }
    } else {
        Ok(Symbol::Unbound.into())
    }
}

/// Return a list of the symbols that typeshed declares in the body scope of
/// the stub for the class `types.ModuleType`.
///
/// Conceptually this could be a `Set` rather than a list,
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

    // `__dict__` and `__init__` are very special members that can be accessed as attributes
    // on the module when imported, but cannot be accessed as globals *inside* the module.
    //
    // `__getattr__` is even more special: it doesn't exist at runtime, but typeshed includes it
    // to reduce false positives associated with functions that dynamically import modules
    // and return `Instance(types.ModuleType)`. We should ignore it for any known module-literal type.
    module_type_symbol_table
        .symbols()
        .filter(|symbol| symbol.is_declared())
        .map(semantic_index::symbol::Symbol::name)
        .filter(|symbol_name| !matches!(&***symbol_name, "__dict__" | "__getattr__" | "__init__"))
        .cloned()
        .collect()
}

/// Return the symbol for a member of `types.ModuleType`.
///
/// ## Notes
///
/// In general we wouldn't check to see whether a symbol exists on a class before doing the
/// [`member`] call on the instance type -- we'd just do the [`member`] call on the instance
/// type, since it has the same end result. The reason to only call [`member`] on [`ModuleType`]
/// instance when absolutely necessary is that it was a fairly significant performance regression
/// to fallback to doing that for every name lookup that wasn't found in the module's globals
/// ([`global_symbol`]). So we use less idiomatic (and much more verbose) code here as a
/// micro-optimisation because it's used in a very hot path.
///
/// [`member`]: Type::member
/// [`ModuleType`]: KnownClass::ModuleType
fn module_type_symbol<'db>(db: &'db dyn Db, name: &str) -> Symbol<'db> {
    if module_type_symbols(db)
        .iter()
        .any(|module_type_member| &**module_type_member == name)
    {
        KnownClass::ModuleType.to_instance(db).member(db, name)
    } else {
        Symbol::Unbound
    }
}

/// Implementation of looking up a module-global symbol as seen from outside the file (e.g. via
/// imports).
///
/// This will take into account whether the definition of the symbol is being explicitly
/// re-exported from a stub file or not.
fn external_symbol_impl<'db>(db: &'db dyn Db, file: File, name: &str) -> Symbol<'db> {
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

        // Start from an unbound symbol
        assert_eq!(
            Symbol::Unbound.or_fall_back_to(&db, || Symbol::Unbound),
            Symbol::Unbound
        );
        assert_eq!(
            Symbol::Unbound.or_fall_back_to(&db, || Symbol::Type(ty1, PossiblyUnbound)),
            Symbol::Type(ty1, PossiblyUnbound)
        );
        assert_eq!(
            Symbol::Unbound.or_fall_back_to(&db, || Symbol::Type(ty1, Bound)),
            Symbol::Type(ty1, Bound)
        );

        // Start from a possibly unbound symbol
        assert_eq!(
            Symbol::Type(ty1, PossiblyUnbound).or_fall_back_to(&db, || Symbol::Unbound),
            Symbol::Type(ty1, PossiblyUnbound)
        );
        assert_eq!(
            Symbol::Type(ty1, PossiblyUnbound)
                .or_fall_back_to(&db, || Symbol::Type(ty2, PossiblyUnbound)),
            Symbol::Type(UnionType::from_elements(&db, [ty1, ty2]), PossiblyUnbound)
        );
        assert_eq!(
            Symbol::Type(ty1, PossiblyUnbound).or_fall_back_to(&db, || Symbol::Type(ty2, Bound)),
            Symbol::Type(UnionType::from_elements(&db, [ty1, ty2]), Bound)
        );

        // Start from a definitely bound symbol
        assert_eq!(
            Symbol::Type(ty1, Bound).or_fall_back_to(&db, || Symbol::Unbound),
            Symbol::Type(ty1, Bound)
        );
        assert_eq!(
            Symbol::Type(ty1, Bound).or_fall_back_to(&db, || Symbol::Type(ty2, PossiblyUnbound)),
            Symbol::Type(ty1, Bound)
        );
        assert_eq!(
            Symbol::Type(ty1, Bound).or_fall_back_to(&db, || Symbol::Type(ty2, Bound)),
            Symbol::Type(ty1, Bound)
        );
    }

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
