use crate::module_name::ModuleName;
use crate::module_resolver::resolve_module;
use crate::semantic_index::global_scope;
use crate::semantic_index::symbol::ScopeId;
use crate::types::{global_symbol_ty, SymbolLookupResult, Type};
use crate::Db;

/// Enumeration of various core stdlib modules, for which we have dedicated Salsa queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoreStdlibModule {
    Builtins,
    Types,
    // the Typing enum is currently only used in tests
    #[allow(dead_code)]
    Typing,
    Typeshed,
    TypingExtensions,
}

impl CoreStdlibModule {
    fn name(self) -> ModuleName {
        let module_name = match self {
            Self::Builtins => "builtins",
            Self::Types => "types",
            Self::Typing => "typing",
            Self::Typeshed => "_typeshed",
            Self::TypingExtensions => "typing_extensions",
        };
        ModuleName::new_static(module_name)
            .unwrap_or_else(|| panic!("{module_name} should be a valid module name!"))
    }
}

/// Lookup the type of `symbol` in a given core module
///
/// Returns `SymbolLookupResult::Unbound` if the given core module cannot be resolved for some reason
fn core_module_symbol_ty<'db>(
    db: &'db dyn Db,
    core_module: CoreStdlibModule,
    symbol: &str,
) -> SymbolLookupResult<'db> {
    resolve_module(db, &core_module.name())
        .map(|module| global_symbol_ty(db, module.file(), symbol))
        .map(|res| {
            if res.is_unbound() {
                res
            } else {
                res.replace_unbound_with(db, Type::Never)
            }
        })
        .unwrap_or(SymbolLookupResult::Unbound)
}

/// Lookup the type of `symbol` in the builtins namespace.
///
/// Returns `SymbolLookupResult::Unbound` if the `builtins` module isn't available for some reason.
#[inline]
pub(crate) fn builtins_symbol_ty<'db>(db: &'db dyn Db, symbol: &str) -> SymbolLookupResult<'db> {
    core_module_symbol_ty(db, CoreStdlibModule::Builtins, symbol)
}

/// Lookup the type of `symbol` in the `types` module namespace.
///
/// Returns `SymbolLookupResult::Unbound` if the `types` module isn't available for some reason.
#[inline]
pub(crate) fn types_symbol_ty<'db>(db: &'db dyn Db, symbol: &str) -> SymbolLookupResult<'db> {
    core_module_symbol_ty(db, CoreStdlibModule::Types, symbol)
}

/// Lookup the type of `symbol` in the `typing` module namespace.
///
/// Returns `SymbolLookupResult::Unbound` if the `typing` module isn't available for some reason.
#[inline]
#[allow(dead_code)] // currently only used in tests
pub(crate) fn typing_symbol_ty<'db>(db: &'db dyn Db, symbol: &str) -> SymbolLookupResult<'db> {
    core_module_symbol_ty(db, CoreStdlibModule::Typing, symbol)
}
/// Lookup the type of `symbol` in the `_typeshed` module namespace.
///
/// Returns `SymbolLookupResult::Unbound` if the `_typeshed` module isn't available for some reason.
#[inline]
pub(crate) fn typeshed_symbol_ty<'db>(db: &'db dyn Db, symbol: &str) -> SymbolLookupResult<'db> {
    core_module_symbol_ty(db, CoreStdlibModule::Typeshed, symbol)
}

/// Lookup the type of `symbol` in the `typing_extensions` module namespace.
///
/// Returns `SymbolLookupResult::Unbound` if the `typing_extensions` module isn't available for some reason.
#[inline]
pub(crate) fn typing_extensions_symbol_ty<'db>(
    db: &'db dyn Db,
    symbol: &str,
) -> SymbolLookupResult<'db> {
    core_module_symbol_ty(db, CoreStdlibModule::TypingExtensions, symbol)
}

/// Get the scope of a core stdlib module.
///
/// Can return `None` if a custom typeshed is used that is missing the core module in question.
fn core_module_scope(db: &dyn Db, core_module: CoreStdlibModule) -> Option<ScopeId<'_>> {
    resolve_module(db, &core_module.name()).map(|module| global_scope(db, module.file()))
}

/// Get the `builtins` module scope.
///
/// Can return `None` if a custom typeshed is used that is missing `builtins.pyi`.
pub(crate) fn builtins_module_scope(db: &dyn Db) -> Option<ScopeId<'_>> {
    core_module_scope(db, CoreStdlibModule::Builtins)
}
