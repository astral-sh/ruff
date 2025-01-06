use crate::module_resolver::{resolve_module, KnownModule};
use crate::semantic_index::global_scope;
use crate::semantic_index::symbol::ScopeId;
use crate::symbol::Symbol;
use crate::types::global_symbol;
use crate::Db;

/// Lookup the type of `symbol` in a given known module
///
/// Returns `Symbol::Unbound` if the given known module cannot be resolved for some reason
pub(crate) fn known_module_symbol<'db>(
    db: &'db dyn Db,
    known_module: KnownModule,
    symbol: &str,
) -> Symbol<'db> {
    resolve_module(db, &known_module.name())
        .map(|module| global_symbol(db, module.file(), symbol))
        .unwrap_or(Symbol::Unbound)
}

/// Lookup the type of `symbol` in the builtins namespace.
///
/// Returns `Symbol::Unbound` if the `builtins` module isn't available for some reason.
#[inline]
pub(crate) fn builtins_symbol<'db>(db: &'db dyn Db, symbol: &str) -> Symbol<'db> {
    known_module_symbol(db, KnownModule::Builtins, symbol)
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

/// Get the scope of a core stdlib module.
///
/// Can return `None` if a custom typeshed is used that is missing the core module in question.
fn core_module_scope(db: &dyn Db, core_module: KnownModule) -> Option<ScopeId<'_>> {
    resolve_module(db, &core_module.name()).map(|module| global_scope(db, module.file()))
}

/// Get the `builtins` module scope.
///
/// Can return `None` if a custom typeshed is used that is missing `builtins.pyi`.
pub(crate) fn builtins_module_scope(db: &dyn Db) -> Option<ScopeId<'_>> {
    core_module_scope(db, KnownModule::Builtins)
}
