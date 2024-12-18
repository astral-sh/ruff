use crate::module_name::ModuleName;
use crate::module_resolver::resolve_module;
use crate::semantic_index::global_scope;
use crate::semantic_index::symbol::ScopeId;
use crate::symbol::Symbol;
use crate::types::global_symbol;
use crate::Db;

/// Enumeration of various core stdlib modules, for which we have dedicated Salsa queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreStdlibModule {
    Builtins,
    Types,
    Typeshed,
    TypingExtensions,
    Typing,
    Sys,
    #[allow(dead_code)]
    Abc, // currently only used in tests
    Collections,
}

impl CoreStdlibModule {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Builtins => "builtins",
            Self::Types => "types",
            Self::Typing => "typing",
            Self::Typeshed => "_typeshed",
            Self::TypingExtensions => "typing_extensions",
            Self::Sys => "sys",
            Self::Abc => "abc",
            Self::Collections => "collections",
        }
    }

    pub(crate) fn name(self) -> ModuleName {
        let self_as_str = self.as_str();
        ModuleName::new_static(self_as_str)
            .unwrap_or_else(|| panic!("{self_as_str} should be a valid module name!"))
    }
}

/// Lookup the type of `symbol` in a given core module
///
/// Returns `Symbol::Unbound` if the given core module cannot be resolved for some reason
pub(crate) fn core_module_symbol<'db>(
    db: &'db dyn Db,
    core_module: CoreStdlibModule,
    symbol: &str,
) -> Symbol<'db> {
    resolve_module(db, &core_module.name())
        .map(|module| global_symbol(db, module.file(), symbol))
        .unwrap_or(Symbol::Unbound)
}

/// Lookup the type of `symbol` in the builtins namespace.
///
/// Returns `Symbol::Unbound` if the `builtins` module isn't available for some reason.
#[inline]
pub(crate) fn builtins_symbol<'db>(db: &'db dyn Db, symbol: &str) -> Symbol<'db> {
    core_module_symbol(db, CoreStdlibModule::Builtins, symbol)
}

/// Lookup the type of `symbol` in the `typing` module namespace.
///
/// Returns `Symbol::Unbound` if the `typing` module isn't available for some reason.
#[inline]
#[cfg(test)]
pub(crate) fn typing_symbol<'db>(db: &'db dyn Db, symbol: &str) -> Symbol<'db> {
    core_module_symbol(db, CoreStdlibModule::Typing, symbol)
}

/// Lookup the type of `symbol` in the `typing_extensions` module namespace.
///
/// Returns `Symbol::Unbound` if the `typing_extensions` module isn't available for some reason.
#[inline]
pub(crate) fn typing_extensions_symbol<'db>(db: &'db dyn Db, symbol: &str) -> Symbol<'db> {
    core_module_symbol(db, CoreStdlibModule::TypingExtensions, symbol)
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
