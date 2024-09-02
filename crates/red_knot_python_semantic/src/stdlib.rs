use crate::module_name::ModuleName;
use crate::module_resolver::resolve_module;
use crate::semantic_index::global_scope;
use crate::semantic_index::symbol::ScopeId;
use crate::types::{symbol_ty_by_name, Type};
use crate::Db;

/// Enumeration of various core stdlib modules, for which we have dedicated Salsa queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreStdlibModule {
    Builtins,
    Types,
    Typeshed,
}

impl CoreStdlibModule {
    /// Retrieve the global scope of the given module.
    ///
    /// Returns `None` if the given module isn't available for some reason.
    pub(crate) fn global_scope(self, db: &dyn Db) -> Option<ScopeId<'_>> {
        match self {
            Self::Builtins => builtins_scope(db),
            Self::Types => types_scope(db),
            Self::Typeshed => typeshed_scope(db),
        }
    }

    /// Shorthand for `symbol_ty` that looks up a symbol in the scope of a given core module.
    ///
    /// Returns `Unbound` if the given module isn't available for some reason.
    pub(crate) fn symbol_ty_by_name<'db>(self, db: &'db dyn Db, name: &str) -> Type<'db> {
        self.global_scope(db)
            .map(|globals| symbol_ty_by_name(db, globals, name))
            .unwrap_or(Type::Unbound)
    }
}

/// Shorthand for `symbol_ty` that looks up a symbol in the `builtins` scope.
///
/// Returns `Unbound` if the `builtins` module isn't available for some reason.
#[inline]
pub(crate) fn builtins_symbol_ty_by_name<'db>(db: &'db dyn Db, name: &str) -> Type<'db> {
    CoreStdlibModule::Builtins.symbol_ty_by_name(db, name)
}

/// Salsa query to get the builtins scope.
///
/// Can return None if a custom typeshed is used that is missing `builtins.pyi`.
#[salsa::tracked]
pub(crate) fn builtins_scope(db: &dyn Db) -> Option<ScopeId<'_>> {
    let builtins_name =
        ModuleName::new_static("builtins").expect("Expected 'builtins' to be a valid module name");
    let builtins_file = resolve_module(db, builtins_name)?.file();
    Some(global_scope(db, builtins_file))
}

/// Salsa query to get the scope for the `types` module.
///
/// Can return None if a custom typeshed is used that is missing `types.pyi`.
#[salsa::tracked]
pub(crate) fn types_scope(db: &dyn Db) -> Option<ScopeId<'_>> {
    let types_module_name =
        ModuleName::new_static("types").expect("Expected 'types' to be a valid module name");
    let types_file = resolve_module(db, types_module_name)?.file();
    Some(global_scope(db, types_file))
}

/// Salsa query to get the scope for the `_typeshed` module.
///
/// Can return None if a custom typeshed is used that is missing a `_typeshed` directory.
#[salsa::tracked]
pub(crate) fn typeshed_scope(db: &dyn Db) -> Option<ScopeId<'_>> {
    let typeshed_module_name = ModuleName::new_static("_typeshed")
        .expect("Expected '_typeshed' to be a valid module name");
    let typeshed_file = resolve_module(db, typeshed_module_name)?.file();
    Some(global_scope(db, typeshed_file))
}
