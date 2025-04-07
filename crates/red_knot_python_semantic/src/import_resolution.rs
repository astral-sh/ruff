use ruff_python_ast as ast;

use crate::{
    module_name::{ModuleName, ModuleNameResolutionError},
    module_resolver::resolve_module,
    semantic_index::{definition::StarImportDefinitionKind, SemanticIndex},
    symbol::SymbolAndQualifiers,
    types::{InferContext, Type},
};

pub(crate) fn resolve_star_import_definition<'db>(
    context: &InferContext<'db>,
    star_import_definition: &StarImportDefinitionKind,
    semantic_index: &SemanticIndex<'db>,
) -> Result<SymbolAndQualifiers<'db>, UnresolvedImportFromError> {
    let import_from = star_import_definition.import();
    let alias = star_import_definition.alias();
    let symbol_id = star_import_definition.symbol_id();

    let (_, module_type) = resolve_import_from_module(context, import_from, alias)?;
    let symbol_table = semantic_index.symbol_table(context.scope().file_scope_id(context.db()));
    let defined_name = symbol_table.symbol(symbol_id).name();
    let imported_symbol = module_type.member(context.db(), defined_name);

    Ok(imported_symbol)
}

/// Resolve the [`ModuleName`], and the type of the module, being referred to by an
/// [`ast::StmtImportFrom`] node. Emit a diagnostic if the module cannot be resolved.
pub(crate) fn resolve_import_from_module<'db>(
    context: &InferContext<'db>,
    import_from: &ast::StmtImportFrom,
    alias: &ast::Alias,
) -> Result<(ModuleName, Type<'db>), UnresolvedImportFromError> {
    let ast::StmtImportFrom { module, level, .. } = import_from;
    let module = module.as_deref();

    let db = context.db();
    let file = context.file();

    tracing::trace!(
        "Resolving imported object `{}` from module `{}` into file `{}`",
        alias.name,
        format_import_from_module(*level, module),
        file.path(db),
    );

    let module_name =
        ModuleName::from_import_statement(db, file, import_from).map_err(|err| match err {
            ModuleNameResolutionError::InvalidSyntax => {
                tracing::debug!("Failed to resolve import due to invalid syntax");
                UnresolvedImportFromError::InvalidSyntax
            }
            ModuleNameResolutionError::TooManyDots => {
                tracing::debug!(
                    "Relative module resolution `{}` failed: too many leading dots",
                    format_import_from_module(*level, module),
                );
                UnresolvedImportFromError::UnresolvedModule
            }
            ModuleNameResolutionError::UnknownCurrentModule => {
                tracing::debug!(
                "Relative module resolution `{}` failed; could not resolve file `{}` to a module",
                format_import_from_module(*level, module),
                file.path(db)
            );
                UnresolvedImportFromError::UnresolvedModule
            }
        })?;

    module_type_from_name(context, &module_name)
        .map(|module_type| (module_name, module_type))
        .ok_or(UnresolvedImportFromError::UnresolvedModule)
}

fn format_import_from_module(level: u32, module: Option<&str>) -> String {
    format!(
        "{}{}",
        ".".repeat(level as usize),
        module.unwrap_or_default()
    )
}

pub(crate) fn module_type_from_name<'db>(
    context: &InferContext<'db>,
    module_name: &ModuleName,
) -> Option<Type<'db>> {
    resolve_module(context.db(), module_name)
        .map(|module| Type::module_literal(context.db(), context.file(), module))
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum UnresolvedImportFromError {
    UnresolvedModule,
    InvalidSyntax,
}

impl UnresolvedImportFromError {
    pub(crate) fn is_invalid_syntax(self) -> bool {
        matches!(self, UnresolvedImportFromError::InvalidSyntax)
    }
}
