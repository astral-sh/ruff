use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};
use ty_module_resolver::{
    ImportingFile, Module, ModuleName, ModuleResolveMode, resolve_module, search_paths,
};

use crate::{
    TypeQualifiers, add_inferred_python_version_hint_to_diagnostic,
    dependency::{DependencyProjectKind, missing_direct_dependency},
    place::{DefinedPlace, Definedness, Place, PlaceAndQualifiers, Provenance, TypeOrigin},
    types::{
        ModuleLiteralType, Type, TypeAndQualifiers,
        diagnostic::{
            MISSING_DIRECT_DEPENDENCY, POSSIBLY_MISSING_IMPORT, UNRESOLVED_IMPORT,
            hint_if_stdlib_attribute_exists_on_other_versions,
            hint_if_stdlib_submodule_exists_on_other_versions,
        },
        infer::TypeInferenceBuilder,
        signatures::effects::legacy_inline,
    },
};
use ty_python_core::definition::Definition;

pub(in crate::types::infer) mod source_effects;
pub(in crate::types::infer) mod statement;

use source_effects::{ImportFromEffects, ImportFromWork, LegacyInlineEffects};

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
    /// Binds an imported value without declaring its type, while preserving inherited `Final`
    /// metadata.
    ///
    /// An import does not itself constrain later assignments. Retaining the source type and
    /// qualifier for imported `Final` values lets the dedicated use-def queries preserve them
    /// through re-exports and reject later reassignment:
    ///
    /// ```python
    /// # values.py
    /// from typing import Final
    /// VALUE: Final[int] = 1
    ///
    /// # consumer.py
    /// from values import VALUE
    /// VALUE = 2  # invalid-assignment
    /// ```
    async fn add_imported_binding_with<E: ImportFromEffects<'db>>(
        &mut self,
        effects: &E,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
        ty: Type<'db>,
        qualifiers: TypeQualifiers,
        provenance: Provenance<'db>,
    ) -> Result<(), E::Error> {
        // Check the imported value before assignment recovery can replace its type.
        if effects
            .definition_kind(self.db(), definition)
            .await?
            .as_star_import()
            .is_none()
        {
            effects.check_deprecated(self, alias.range(), ty).await?;
        }

        effects.insert_binding(self, alias, definition, ty).await?;

        if qualifiers.contains(TypeQualifiers::FINAL) {
            effects.checkpoint(ImportFromWork::FinalDeclaration).await?;
            self.declarations.insert(
                definition,
                TypeAndQualifiers::new(ty, TypeOrigin::Declared, qualifiers)
                    .with_provenance(provenance),
            );
        }
        Ok(())
    }

    pub(super) fn infer_import_statement(&mut self, import: &ast::StmtImport) {
        let ast::StmtImport {
            names,
            is_lazy: _,
            range: _,
            node_index: _,
        } = import;

        for alias in names {
            self.infer_definition(alias);
        }
    }

    async fn check_direct_dependency_with<E: ImportFromEffects<'db>>(
        &self,
        effects: &E,
        module: Module<'db>,
        range: TextRange,
    ) -> Result<(), E::Error> {
        if !effects.direct_dependency_lint_enabled(self).await?
            || effects.direct_dependency_in_stub(self).await?
        {
            return Ok(());
        }

        effects
            .check_direct_dependency_tail(self, module, range)
            .await
    }

    fn check_direct_dependency_tail(&self, module: Module<'db>, range: TextRange) {
        if self.is_in_type_checking_block(self.scope(), range)
            || self
                .settings()
                .replace_imports_with_any
                .matches(module.name(self.db()))
                .is_include()
        {
            return;
        }

        let db = self.db();
        let Some(missing) = missing_direct_dependency(db, self.program_file(), module) else {
            return;
        };

        let Some(builder) = self.context.report_lint(&MISSING_DIRECT_DEPENDENCY, range) else {
            return;
        };

        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Import of `{}` requires a direct dependency on `{}`",
            module.name(db),
            missing.distribution_name,
        ));
        match missing.project_kind {
            DependencyProjectKind::Project => diagnostic.help(format_args!(
                "Declare `{}` in `project.dependencies` or `project.optional-dependencies` in your `pyproject.toml`",
                missing.distribution_name,
            )),
            DependencyProjectKind::Script => diagnostic.help(format_args!(
                "Declare `{}` in the script's inline `dependencies` metadata",
                missing.distribution_name,
            )),
        }
        if missing.group_dependency {
            diagnostic.info("Dependency groups are only available to non-package files");
        }
        diagnostic.info(match missing.project_kind {
            DependencyProjectKind::Project => {
                "See https://docs.astral.sh/uv/concepts/projects/dependencies/"
            }
            DependencyProjectKind::Script => {
                "See https://docs.astral.sh/uv/guides/scripts/#declaring-script-dependencies"
            }
        });
    }

    fn report_unresolved_import(
        &self,
        range: TextRange,
        level: u32,
        module: Option<&str>,
        module_name: Option<&ModuleName>,
    ) {
        let db = self.db();

        if let Some(module_name) = &module_name
            && (self
                .settings()
                .allowed_unresolved_imports
                .matches(module_name)
                .is_include()
                || self
                    .settings()
                    .replace_imports_with_any
                    .matches(module_name)
                    .is_include())
        {
            return;
        }

        let Some(builder) = self.context.report_lint(&UNRESOLVED_IMPORT, range) else {
            return;
        };

        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Cannot resolve imported module `{}`",
            format_import_from_module(level, module)
        ));

        if level == 0 {
            if let Some(module_name) = module_name {
                let resolver_environment = self.program_environment().program(db);
                let typeshed_versions = resolver_environment.search_paths(db).typeshed_versions();

                // Loop over ancestors in case we have info on the parent module but not submodule
                for module_name in module_name.ancestors() {
                    if let Some(version_range) = typeshed_versions.exact(&module_name) {
                        // We know it is a stdlib module on *some* Python versions...
                        let python_version = self.program_environment().python_version(db);
                        if !version_range.contains(python_version) {
                            // ...But not on *this* Python version.
                            diagnostic.info(format_args!(
                                "The stdlib module `{module_name}` is only available on Python {version_range}",
                                version_range = version_range.diagnostic_display(),
                            ));
                            add_inferred_python_version_hint_to_diagnostic(
                                db,
                                self.file(),
                                &mut diagnostic,
                                "resolving modules",
                            );
                            return;
                        }
                        // We found the most precise answer we could, stop searching
                        break;
                    }
                }
            }
        } else {
            let importing_file = ImportingFile::File(
                self.file(),
                self.program_environment().resolver_environment(db),
            );
            if let Some(better_level) = (0..level).rev().find(|reduced_level| {
                let Ok(module_name) =
                    ModuleName::from_identifier_parts(db, importing_file, module, *reduced_level)
                else {
                    return false;
                };
                resolve_module(db, importing_file, &module_name).is_some()
            }) {
                diagnostic
                    .help("The module can be resolved if the number of leading dots is reduced");
                diagnostic.help(format_args!(
                    "Did you mean `{}`?",
                    format_import_from_module(better_level, module)
                ));
                diagnostic.set_concise_message(format_args!(
                    "Cannot resolve imported module `{}` - did you mean `{}`?",
                    format_import_from_module(level, module),
                    format_import_from_module(better_level, module)
                ));
            }
        }

        // Add search paths information to the diagnostic
        // Use the same search paths function that is used in actual module resolution
        let verbose = db.verbose();
        let search_paths = search_paths(
            db,
            self.program_environment().resolver_environment(db),
            ModuleResolveMode::Typing,
        );

        diagnostic.info(format_args!(
            "Searched in the following paths during module resolution:"
        ));

        let mut search_paths = search_paths.enumerate().peekable();

        while let Some((index, path)) = search_paths.next() {
            if index > 4 && !verbose && search_paths.peek().is_some() {
                let more = search_paths.count() + 1;
                diagnostic.info(format_args!(
                    "  ... and {more} more paths. Run with `-v` to see all paths."
                ));
                break;
            }
            diagnostic.info(format_args!(
                "  {}. {} ({})",
                index + 1,
                path,
                path.describe_kind()
            ));
        }

        diagnostic.info(
            "make sure your Python environment is properly configured: \
                https://docs.astral.sh/ty/modules/#python-environment",
        );
    }

    pub(super) fn infer_import_definition(
        &mut self,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
    ) {
        legacy_inline(self.infer_import_definition_with(&LegacyInlineEffects, alias, definition));
    }

    pub(in crate::types::infer) async fn infer_import_definition_with<E: ImportFromEffects<'db>>(
        &mut self,
        effects: &E,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
    ) -> Result<(), E::Error> {
        let ast::Alias {
            range: _,
            node_index: _,
            name,
            asname,
        } = alias;

        // The name of the module being imported
        effects
            .checkpoint(ImportFromWork::FullModuleName { bytes: name.len() })
            .await?;
        let Some(full_module_name) = ModuleName::new(name) else {
            effects.log_invalid_import_syntax().await?;
            effects
                .insert_binding(self, alias, definition, Type::unknown())
                .await?;
            return Ok(());
        };

        if effects
            .replace_import_with_any(self, &full_module_name)
            .await?
        {
            effects
                .insert_binding(self, alias, definition, Type::any())
                .await?;
            return Ok(());
        }

        // Resolve the module being imported.
        let Some(full_module) = effects.resolve_module(self, &full_module_name).await? else {
            effects
                .report_unresolved_plain_import(self, alias, &full_module_name)
                .await?;
            effects
                .insert_binding(self, alias, definition, Type::unknown())
                .await?;
            return Ok(());
        };

        let full_module_ty = Type::ModuleLiteral(effects.module_literal(self, full_module).await?);
        self.check_direct_dependency_with(effects, full_module, alias.range())
            .await?;

        let binding_ty = if asname.is_some() {
            // If we are renaming the imported module via an `as` clause, then we bind the resolved
            // module's type to that name, even if that module is nested.
            full_module_ty
        } else {
            effects
                .checkpoint(ImportFromWork::TopmostParentName {
                    bytes: full_module_name.as_str().len(),
                })
                .await?;
            if full_module_name.contains('.') {
                // If there's no `as` clause and the imported module is nested, we're not going to bind
                // the resolved module itself into the current scope; we're going to bind the top-most
                // parent package of that module.
                let topmost_parent_name =
                    ModuleName::new(full_module_name.first_component()).unwrap();
                let Some(topmost_parent) =
                    effects.resolve_module(self, &topmost_parent_name).await?
                else {
                    effects
                        .insert_binding(self, alias, definition, Type::unknown())
                        .await?;
                    return Ok(());
                };
                Type::ModuleLiteral(effects.module_literal(self, topmost_parent).await?)
            } else {
                // If there's no `as` clause and the imported module isn't nested, then the imported
                // module _is_ what we bind into the current scope.
                full_module_ty
            }
        };

        effects
            .insert_binding(self, alias, definition, binding_ty)
            .await
    }

    pub(super) fn infer_import_from_statement(&mut self, import: &ast::StmtImportFrom) {
        legacy_inline(self.infer_import_from_statement_with(&LegacyInlineEffects, import));
    }

    pub(super) fn infer_import_from_definition(
        &mut self,
        import_from: &ast::StmtImportFrom,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
    ) {
        legacy_inline(self.infer_import_from_definition_with(
            &LegacyInlineEffects,
            import_from,
            alias,
            definition,
        ));
    }

    pub(in crate::types::infer) async fn infer_import_from_definition_with<
        E: ImportFromEffects<'db>,
    >(
        &mut self,
        effects: &E,
        import_from: &ast::StmtImportFrom,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
    ) -> Result<(), E::Error> {
        let db = self.db();

        let Ok(module_name) = effects.module_name(self, import_from).await? else {
            effects
                .insert_binding(self, alias, definition, Type::unknown())
                .await?;
            return Ok(());
        };

        if effects.replace_import_with_any(self, &module_name).await? {
            effects
                .insert_binding(self, alias, definition, Type::any())
                .await?;
            return Ok(());
        }

        let Some(module) = effects.resolve_module(self, &module_name).await? else {
            effects
                .insert_binding(self, alias, definition, Type::unknown())
                .await?;
            return Ok(());
        };

        let module_literal = effects.module_literal(self, module).await?;

        let name = if let Some(star_import) = effects
            .definition_kind(db, definition)
            .await?
            .as_star_import()
        {
            self.index
                .place_table(effects.file_scope_id(db, self.scope()).await?)
                .symbol(star_import.symbol_id())
                .name()
        } else {
            &alias.name.id
        };
        effects
            .checkpoint(ImportFromWork::ImportedName { bytes: name.len() })
            .await?;
        let name = name.clone();

        // Avoid looking up attributes on a module if a module imports from itself
        // at the module-global scope, where the import definition itself is one of the
        // bindings for the symbol being looked up, which would cause a query cycle.
        //
        // In nested scopes (e.g. function bodies), the module's global-scope definitions
        // are resolved independently, so there is no cycle risk and the lookup is safe.
        let skip_self_referential_member_lookup = Some(self.file())
            == effects.module_literal_file(db, module_literal).await?
            && effects.file_scope_id(db, self.scope()).await?.is_global();

        // Although it isn't the runtime semantics, we go to some trouble to prioritize a submodule
        // over module `__getattr__`, because that's what other type checkers do.
        let mut from_module_getattr = None;

        // First try loading the requested attribute from the module.
        if !skip_self_referential_member_lookup {
            let result = effects.static_member(self, module_literal, &name).await?;
            let error = result.err();
            if let PlaceAndQualifiers {
                place:
                    Place::Defined(DefinedPlace {
                        ty,
                        definedness: boundness,
                        provenance: source_provenance,
                        ..
                    }),
                qualifiers,
            } = result
                .unwrap_or_else(|error| error.fallback_member(db))
                .member(db)
            {
                if &alias.name != "*" && boundness == Definedness::PossiblyUndefined {
                    // TODO: Consider loading _both_ the attribute and any submodule and unioning them
                    // together if the attribute exists but is possibly-unbound.
                    effects
                        .checkpoint(ImportFromWork::PossiblyMissingDiagnostic {
                            module_bytes: module_name.as_str().len(),
                            member_bytes: name.len(),
                        })
                        .await?;
                    if let Some(builder) = self
                        .context
                        .report_lint(&POSSIBLY_MISSING_IMPORT, ast::AnyNodeRef::Alias(alias))
                    {
                        builder.into_diagnostic(format_args!(
                            "Member `{name}` of module `{module_name}` may be missing",
                        ));
                    }
                }
                if qualifiers.contains(TypeQualifiers::FROM_MODULE_GETATTR) {
                    from_module_getattr = Some((ty, qualifiers, source_provenance, error));
                } else {
                    self.add_imported_binding_with(
                        effects,
                        alias,
                        definition,
                        ty,
                        qualifiers,
                        source_provenance,
                    )
                    .await?;
                    return Ok(());
                }
            }
        }

        // Evaluate whether `X.Y` would constitute a valid submodule name,
        // given a `from X import Y` statement. If it is valid, this will be `Some()`;
        // else, it will be `None`.
        effects
            .checkpoint(ImportFromWork::SubmoduleName {
                module_bytes: module_name.as_str().len(),
                member_bytes: name.len(),
            })
            .await?;
        let full_submodule_name = ModuleName::new(&name).map(|final_part| {
            let mut ret = module_name.clone();
            ret.extend(&final_part);
            ret
        });

        // If the module doesn't bind the symbol, check if it's a submodule.  This won't get
        // handled by the `Type::member` call because it relies on the semantic index's
        // `imported_modules` set.  The semantic index does not include information about
        // `from...import` statements because there are two things it cannot determine while only
        // inspecting the content of the current file:
        //
        //   - whether the imported symbol is an attribute or submodule
        //   - whether the containing file is in a module or a package (needed to correctly resolve
        //     relative imports)
        //
        // The first would be solvable by making it a _potentially_ imported modules set.  The
        // second is not.
        //
        // Regardless, for now, we sidestep all of that by repeating the submodule-or-attribute
        // check here when inferring types for a `from...import` statement.
        if let Some(submodule_name) = full_submodule_name.as_ref()
            && let Some(submodule_type) = effects.submodule_type(self, submodule_name).await?
        {
            effects
                .insert_binding(self, alias, definition, submodule_type)
                .await?;
            return Ok(());
        }

        // We've checked for a submodule, so now we can go ahead and use a type from module
        // `__getattr__`.
        if let Some((ty, qualifiers, source_provenance, error)) = from_module_getattr {
            if let Some(error) = error {
                effects
                    .report_getattr_error(self, error, module_literal, alias, &name)
                    .await?;
            }
            self.add_imported_binding_with(
                effects,
                alias,
                definition,
                ty,
                qualifiers,
                source_provenance,
            )
            .await?;
            return Ok(());
        }

        effects
            .insert_binding(self, alias, definition, Type::unknown())
            .await?;

        if &alias.name == "*" {
            return Ok(());
        }

        if self
            .settings()
            .allowed_unresolved_imports
            .matches(full_submodule_name.as_ref().unwrap_or(&module_name))
            .is_include()
        {
            return Ok(());
        }

        effects
            .report_missing_import(
                self,
                module_literal,
                &module_name,
                &name,
                alias,
                full_submodule_name.as_ref(),
            )
            .await?;
        Ok(())
    }

    fn report_missing_import_member(
        &self,
        module_literal: ModuleLiteralType<'db>,
        module_name: &ModuleName,
        name: &str,
        alias: &ast::Alias,
        full_submodule_name: Option<&ModuleName>,
    ) {
        let db = self.db();
        let Some(builder) = self
            .context
            .report_lint(&UNRESOLVED_IMPORT, ast::AnyNodeRef::Alias(alias))
        else {
            return;
        };

        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Module `{module_name}` has no member `{name}`"
        ));

        let mut submodule_hint_added = false;

        if let Some(full_submodule_name) = full_submodule_name {
            submodule_hint_added = hint_if_stdlib_submodule_exists_on_other_versions(
                db,
                self.file(),
                self.program_environment(),
                &mut diagnostic,
                full_submodule_name,
                module_literal.module(db),
            );
        }

        if !submodule_hint_added {
            hint_if_stdlib_attribute_exists_on_other_versions(
                db,
                self.program_file(),
                diagnostic,
                Type::ModuleLiteral(module_literal),
                name,
                "resolving imports",
            );
        }
    }

    /// Infer the implicit local definition `x = <module 'whatever.thispackage.x'>` that
    /// `from .x.y import z` or `from whatever.thispackage.x.y` can introduce in `__init__.py(i)`.
    ///
    /// For the definition `z`, see [`TypeInferenceBuilder::infer_import_from_definition`].
    ///
    /// The runtime semantic of this kind of statement is to introduce a variable in the global
    /// scope of this module *the first time it's imported in the entire program*. This
    /// implementation just blindly introduces a local variable wherever the `from..import` is
    /// (if the imports actually resolve).
    ///
    /// That gap between the semantics and implementation are currently the responsibility of the
    /// code that actually creates these kinds of Definitions (so blindly introducing a local
    /// is all we need to be doing here).
    pub(super) fn infer_import_from_submodule_definition(
        &mut self,
        import_from: &'ast ast::StmtImportFrom,
        definition: Definition<'db>,
    ) {
        let db = self.db();
        let importing_file = ImportingFile::File(
            self.file(),
            self.program_environment().resolver_environment(db),
        );

        // Get this package's absolute module name by resolving `.`, and make sure it exists
        let Ok(thispackage_name) = ModuleName::package_for_file(db, importing_file) else {
            self.add_binding(import_from.into(), definition)
                .insert(self, Type::unknown());
            return;
        };

        let Some(module) = resolve_module(db, importing_file, &thispackage_name) else {
            self.add_binding(import_from.into(), definition)
                .insert(self, Type::unknown());
            return;
        };

        // We have `from whatever.thispackage.x.y ...` or `from .x.y ...`
        // and we want to extract `x` (to ultimately construct `whatever.thispackage.x`):

        // First we normalize to `whatever.thispackage.x.y`
        let Some(final_part) = ModuleName::from_identifier_parts(
            db,
            importing_file,
            import_from.module.as_deref(),
            import_from.level,
        )
        .ok()
        // `whatever.thispackage.x.y` => `x.y`
        .and_then(|submodule_name| submodule_name.relative_to(&thispackage_name))
        // `x.y` => `x`
        .and_then(|relative_submodule_name| {
            relative_submodule_name
                .components()
                .next()
                .and_then(ModuleName::new)
        }) else {
            self.add_binding(import_from.into(), definition)
                .insert(self, Type::unknown());
            return;
        };

        // `x` => `whatever.thispackage.x`
        let mut full_submodule_name = thispackage_name.clone();
        full_submodule_name.extend(&final_part);

        // Try to actually resolve the import `whatever.thispackage.x`
        if let Some(submodule_type) = self.module_type_from_name(&full_submodule_name) {
            // Success, introduce a binding!
            //
            // We explicitly don't introduce a *declaration* because it's actual ok
            // (and fairly common) to overwrite this import with a function or class
            // and we don't want it to be a type error to do so.
            self.add_binding(import_from.into(), definition)
                .insert(self, submodule_type);
            return;
        }

        // That didn't work, try to produce diagnostics
        self.add_binding(import_from.into(), definition)
            .insert(self, Type::unknown());

        if self
            .settings()
            .allowed_unresolved_imports
            .matches(&full_submodule_name)
            .is_include()
        {
            return;
        }

        let Some(builder) = self.context.report_lint(
            &UNRESOLVED_IMPORT,
            ast::AnyNodeRef::StmtImportFrom(import_from),
        ) else {
            return;
        };

        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Module `{thispackage_name}` has no submodule `{final_part}`"
        ));

        hint_if_stdlib_submodule_exists_on_other_versions(
            self.db(),
            self.file(),
            self.program_environment(),
            &mut diagnostic,
            &full_submodule_name,
            module,
        );
    }
}

fn format_import_from_module(level: u32, module: Option<&str>) -> String {
    format!(
        "{}{}",
        ".".repeat(level as usize),
        module.unwrap_or_default()
    )
}
