//! Shared `from`-import statement traversal and module-resolution diagnostics.

use std::future::{Future, ready};

use ruff_python_ast as ast;
use ruff_text_size::Ranged;
use ty_module_resolver::{Module, ModuleName, ModuleNameResolutionError};
use ty_python_core::definition::Definition;
use ty_python_core::symbol::ScopedSymbolId;

use super::source_effects::{ImportFromEffects, LegacyInlineEffects};
use super::{TypeInferenceBuilder, format_import_from_module};
use crate::reachability::evaluate_reachability_with_cache;
use crate::types::infer::DefinitionInference;
use crate::types::{ModuleLiteralType, Type, infer_definition_types};

pub(in crate::types::infer) fn module_matches_imported_child(
    parent: &ModuleName,
    child: &ModuleName,
    name: &str,
) -> bool {
    child.parent().as_ref() == Some(parent) && child.components().next_back() == Some(name)
}

pub(in crate::types::infer) trait ImportStatementEffects<'db>:
    ImportFromEffects<'db>
{
    async fn log_module_resolution(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        import: &ast::StmtImportFrom,
    ) -> Result<(), Self::Error>;

    async fn log_module_name_error(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        import: &ast::StmtImportFrom,
        error: &ModuleNameResolutionError,
    ) -> Result<(), Self::Error>;

    async fn report_unresolved_from_import(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        import: &ast::StmtImportFrom,
        module_name: Option<&ModuleName>,
    ) -> Result<(), Self::Error>;

    async fn next_alias<'ast>(
        &self,
        import: &'ast ast::StmtImportFrom,
        cursor: &mut usize,
    ) -> Result<Option<&'ast ast::Alias>, Self::Error>;

    async fn definitions(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        alias: &ast::Alias,
    ) -> Result<&'db [Definition<'db>], Self::Error>;

    async fn next_definition(
        &self,
        definitions: &[Definition<'db>],
        cursor: &mut usize,
    ) -> Result<Option<Definition<'db>>, Self::Error>;

    async fn star_import_is_unreachable(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<bool, Self::Error>;

    async fn definition(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionInference<'db>, Self::Error>;

    async fn bindings<'inference>(
        &self,
        inference: &'inference DefinitionInference<'db>,
        definition: Definition<'db>,
    ) -> Result<impl ExactSizeIterator<Item = (Definition<'db>, Type<'db>)> + 'inference, Self::Error>;

    async fn next_binding(
        &self,
        bindings: &mut impl ExactSizeIterator<Item = (Definition<'db>, Type<'db>)>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn module_literal_module(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        literal: ModuleLiteralType<'db>,
    ) -> Result<Module<'db>, Self::Error>;

    async fn module_matches_imported_child(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        parent: Module<'db>,
        child: Module<'db>,
        alias: &ast::Alias,
    ) -> Result<bool, Self::Error>;

    async fn extend_definition(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        inference: &DefinitionInference<'db>,
    ) -> Result<(), Self::Error>;
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(in crate::types::infer) async fn infer_import_from_statement_with<
        E: ImportStatementEffects<'db>,
    >(
        &mut self,
        effects: &E,
        import: &ast::StmtImportFrom,
    ) -> Result<(), E::Error> {
        let module = self
            .check_import_from_module_is_resolvable_with(effects, import)
            .await?;
        let import_range = import.module.as_ref().map_or(import.range(), Ranged::range);

        let mut alias_cursor = 0;
        while let Some(alias) = effects.next_alias(import, &mut alias_cursor).await? {
            let mut checked_dependency = false;
            let definitions = effects.definitions(self, alias).await?;
            let mut definition_cursor = 0;
            while let Some(definition) = effects
                .next_definition(definitions, &mut definition_cursor)
                .await?
            {
                let kind = effects.definition_kind(self.db(), definition).await?;
                if let Some(star_import) = kind.as_star_import()
                    && effects
                        .star_import_is_unreachable(self, definition, star_import.symbol_id())
                        .await?
                {
                    continue;
                }

                let inferred = effects.definition(self, definition).await?;
                // Check non-star imports for missing direct dependencies.
                if kind.as_star_import().is_none() {
                    // Cycle recovery can omit bindings; the fallback below checks the parent module.
                    let mut bindings = effects.bindings(inferred, definition).await?;
                    while let Some(ty) = effects.next_binding(&mut bindings).await? {
                        // `from namespace import child` can import a distribution other than the
                        // namespace's other children. Use inference's attribute-versus-submodule
                        // decision, and do not follow values re-exported from unrelated modules.
                        if effects.direct_dependency_lint_enabled(self).await?
                            && let Some(parent) = module
                        {
                            let imported_module = if let Type::ModuleLiteral(literal) = ty {
                                let child = effects.module_literal_module(self, literal).await?;
                                if effects
                                    .module_matches_imported_child(self, parent, child, alias)
                                    .await?
                                {
                                    child
                                } else {
                                    parent
                                }
                            } else {
                                parent
                            };
                            self.check_direct_dependency_with(
                                effects,
                                imported_module,
                                import_range,
                            )
                            .await?;
                            checked_dependency = true;
                        }
                    }
                }
                effects
                    .extend_definition(self, definition, inferred)
                    .await?;
            }

            // Star imports can have no definitions, and cycle recovery can omit bindings.
            if !checked_dependency && let Some(parent) = module {
                self.check_direct_dependency_with(effects, parent, import_range)
                    .await?;
            }
        }
        Ok(())
    }

    /// Resolve and return the module referred to by the `from` clause of an
    /// [`ast::StmtImportFrom`] node. For `from package import child`, this returns
    /// `package`, not `child`. Relative imports are resolved to an absolute module name.
    ///
    /// Return `None` if the module name is invalid or the module cannot be resolved.
    /// Emit an unresolved-import diagnostic for resolution failures; syntax errors are
    /// reported elsewhere.
    async fn check_import_from_module_is_resolvable_with<E: ImportStatementEffects<'db>>(
        &self,
        effects: &E,
        import: &ast::StmtImportFrom,
    ) -> Result<Option<Module<'db>>, E::Error> {
        effects.log_module_resolution(self, import).await?;
        let module_name = match effects.module_name(self, import).await? {
            Ok(module_name) => module_name,
            Err(error) => {
                effects.log_module_name_error(self, import, &error).await?;
                if !matches!(error, ModuleNameResolutionError::InvalidSyntax) {
                    effects
                        .report_unresolved_from_import(self, import, None)
                        .await?;
                }
                // Invalid syntax diagnostics are emitted elsewhere.
                return Ok(None);
            }
        };

        let resolved = effects.resolve_module(self, &module_name).await?;
        if resolved.is_none() {
            effects
                .report_unresolved_from_import(self, import, Some(&module_name))
                .await?;
        }
        Ok(resolved)
    }
}

impl<'db> ImportStatementEffects<'db> for LegacyInlineEffects {
    fn log_module_resolution(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        import: &ast::StmtImportFrom,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        tracing::trace!(
            "Resolving import statement from module `{}` into file `{}`",
            format_import_from_module(import.level, import.module.as_deref()),
            builder.file().path(builder.db()),
        );
        ready(Ok(()))
    }

    fn log_module_name_error(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        import: &ast::StmtImportFrom,
        error: &ModuleNameResolutionError,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        match error {
            ModuleNameResolutionError::InvalidSyntax => {
                tracing::debug!("Failed to resolve import due to invalid syntax");
            }
            ModuleNameResolutionError::TooManyDots => {
                tracing::debug!(
                    "Relative module resolution `{}` failed: too many leading dots",
                    format_import_from_module(import.level, import.module.as_deref()),
                );
            }
            ModuleNameResolutionError::UnknownCurrentModule => {
                tracing::debug!(
                    "Relative module resolution `{}` failed: could not resolve file `{}` to a module \
                    (try adjusting configured search paths?)",
                    format_import_from_module(import.level, import.module.as_deref()),
                    builder.file().path(builder.db())
                );
            }
        }
        ready(Ok(()))
    }

    fn report_unresolved_from_import(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        import: &ast::StmtImportFrom,
        module_name: Option<&ModuleName>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        // For diagnostics, we want to highlight the unresolvable
        // module and not the entire `from ... import ...` statement.
        let module_ref = import
            .module
            .as_ref()
            .map(ast::AnyNodeRef::from)
            .unwrap_or_else(|| ast::AnyNodeRef::from(import));
        builder.report_unresolved_import(
            module_ref.range(),
            import.level,
            import.module.as_deref(),
            module_name,
        );
        ready(Ok(()))
    }

    fn next_alias<'ast>(
        &self,
        import: &'ast ast::StmtImportFrom,
        cursor: &mut usize,
    ) -> impl Future<Output = Result<Option<&'ast ast::Alias>, Self::Error>> {
        let next = import.names.get(*cursor);
        *cursor += usize::from(next.is_some());
        ready(Ok(next))
    }

    fn definitions(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        alias: &ast::Alias,
    ) -> impl Future<Output = Result<&'db [Definition<'db>], Self::Error>> {
        ready(Ok(builder.index.definitions(alias)))
    }

    fn next_definition(
        &self,
        definitions: &[Definition<'db>],
        cursor: &mut usize,
    ) -> impl Future<Output = Result<Option<Definition<'db>>, Self::Error>> {
        let next = definitions.get(*cursor).copied();
        *cursor += usize::from(next.is_some());
        ready(Ok(next))
    }

    fn star_import_is_unreachable(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        symbol: ScopedSymbolId,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        let db = builder.db();
        let use_def = builder.index.use_def_map(builder.scope().file_scope_id(db));
        let unreachable = use_def
            .reachable_symbol_bindings(symbol)
            .find(|binding| {
                binding
                    .binding
                    .is_defined_and(|candidate| candidate == definition)
            })
            .is_some_and(|binding| {
                evaluate_reachability_with_cache(
                    db,
                    Some(builder.reachability_cache()),
                    use_def.reachability_constraints(),
                    use_def.predicates(),
                    binding.reachability_constraint,
                )
                .is_always_false()
            });
        ready(Ok(unreachable))
    }

    fn definition(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<&'db DefinitionInference<'db>, Self::Error>> {
        ready(Ok(infer_definition_types(builder.db(), definition)))
    }

    async fn bindings<'inference>(
        &self,
        inference: &'inference DefinitionInference<'db>,
        definition: Definition<'db>,
    ) -> Result<impl ExactSizeIterator<Item = (Definition<'db>, Type<'db>)> + 'inference, Self::Error>
    {
        Ok(inference.bindings(definition))
    }

    fn next_binding(
        &self,
        bindings: &mut impl ExactSizeIterator<Item = (Definition<'db>, Type<'db>)>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        ready(Ok(bindings.next().map(|(_, ty)| ty)))
    }

    fn module_literal_module(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        literal: ModuleLiteralType<'db>,
    ) -> impl Future<Output = Result<Module<'db>, Self::Error>> {
        ready(Ok(literal.module(builder.db())))
    }

    fn module_matches_imported_child(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        parent: Module<'db>,
        child: Module<'db>,
        alias: &ast::Alias,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        let db = builder.db();
        let child_name = child.name(db);
        let parent_name = parent.name(db);
        ready(Ok(module_matches_imported_child(
            parent_name,
            child_name,
            alias.name.as_str(),
        )))
    }

    fn extend_definition(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        inference: &DefinitionInference<'db>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        builder.extend_definition(definition, inference);
        ready(Ok(()))
    }
}
