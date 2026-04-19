use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};
use ty_module_resolver::{
    ModuleName, ModuleNameResolutionError, ModuleResolveMode, resolve_module, search_paths,
};

use crate::{
    Program, TypeQualifiers, add_inferred_python_version_hint_to_diagnostic,
    place::{DefinedPlace, Definedness, Place, PlaceAndQualifiers, TypeOrigin},
    types::{
        Type, TypeAndQualifiers,
        diagnostic::{
            POSSIBLY_MISSING_IMPORT, UNRESOLVED_IMPORT,
            hint_if_stdlib_attribute_exists_on_other_versions,
            hint_if_stdlib_submodule_exists_on_other_versions,
        },
        infer::{TypeInferenceBuilder, builder::DeclaredAndInferredType},
        infer_definition_types,
    },
};
use ty_python_core::definition::Definition;

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
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
                let program = Program::get(db);
                let typeshed_versions = program.search_paths(db).typeshed_versions();

                // Loop over ancestors in case we have info on the parent module but not submodule
                for module_name in module_name.ancestors() {
                    if let Some(version_range) = typeshed_versions.exact(&module_name) {
                        // We know it is a stdlib module on *some* Python versions...
                        let python_version = program.python_version(db);
                        if !version_range.contains(python_version) {
                            // ...But not on *this* Python version.
                            diagnostic.info(format_args!(
                                "The stdlib module `{module_name}` is only available on Python {version_range}",
                                version_range = version_range.diagnostic_display(),
                            ));
                            add_inferred_python_version_hint_to_diagnostic(
                                db,
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
            if let Some(better_level) = (0..level).rev().find(|reduced_level| {
                let Ok(module_name) =
                    ModuleName::from_identifier_parts(db, self.file(), module, *reduced_level)
                else {
                    return false;
                };
                resolve_module(db, self.file(), &module_name).is_some()
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
        let search_paths = search_paths(db, ModuleResolveMode::StubsAllowed);

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
        alias: &ast::Alias,
        definition: Definition<'db>,
    ) {
        let ast::Alias {
            range: _,
            node_index: _,
            name,
            asname,
        } = alias;

        // The name of the module being imported
        let Some(full_module_name) = ModuleName::new(name) else {
            tracing::debug!("Failed to resolve import due to invalid syntax");
            self.add_unknown_declaration_with_binding(alias.into(), definition);
            return;
        };

        if self
            .settings()
            .replace_imports_with_any
            .matches(&full_module_name)
            .is_include()
        {
            self.add_declaration_with_binding(
                alias.into(),
                definition,
                &DeclaredAndInferredType::are_the_same_type(Type::any()),
            );
            return;
        }

        // Resolve the module being imported.
        let Some(full_module_ty) = self.module_type_from_name(&full_module_name) else {
            self.report_unresolved_import(alias.range(), 0, Some(name), Some(&full_module_name));
            self.add_unknown_declaration_with_binding(alias.into(), definition);
            return;
        };

        let binding_ty = if asname.is_some() {
            // If we are renaming the imported module via an `as` clause, then we bind the resolved
            // module's type to that name, even if that module is nested.
            full_module_ty
        } else if full_module_name.contains('.') {
            // If there's no `as` clause and the imported module is nested, we're not going to bind
            // the resolved module itself into the current scope; we're going to bind the top-most
            // parent package of that module.
            let topmost_parent_name =
                ModuleName::new(full_module_name.components().next().unwrap()).unwrap();
            let Some(topmost_parent_ty) = self.module_type_from_name(&topmost_parent_name) else {
                self.add_unknown_declaration_with_binding(alias.into(), definition);
                return;
            };
            topmost_parent_ty
        } else {
            // If there's no `as` clause and the imported module isn't nested, then the imported
            // module _is_ what we bind into the current scope.
            full_module_ty
        };

        self.add_declaration_with_binding(
            alias.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(binding_ty),
        );
    }

    pub(super) fn infer_import_from_statement(&mut self, import: &ast::StmtImportFrom) {
        let ast::StmtImportFrom {
            module: _,
            names,
            level: _,
            is_lazy: _,
            range: _,
            node_index: _,
        } = import;

        let db = self.db();

        self.check_import_from_module_is_resolvable(import);

        for alias in names {
            for definition in self.index.definitions(alias) {
                let inferred = infer_definition_types(db, *definition);
                // Check non-star imports for deprecations
                if definition.kind(db).as_star_import().is_none() {
                    // In the initial cycle, `declaration_types()` is empty, so no deprecation check is performed.
                    for ty in inferred.declaration_types() {
                        self.check_deprecated(alias, ty.inner);
                    }
                }
                self.extend_definition(inferred);
            }
        }
    }

    /// Resolve the [`ModuleName`], and the type of the module, being referred to by an
    /// [`ast::StmtImportFrom`] node. Emit a diagnostic if the module cannot be resolved.
    fn check_import_from_module_is_resolvable(&mut self, import_from: &ast::StmtImportFrom) {
        let ast::StmtImportFrom { module, level, .. } = import_from;

        let db = self.db();

        // For diagnostics, we want to highlight the unresolvable
        // module and not the entire `from ... import ...` statement.
        let module_ref = module
            .as_ref()
            .map(ast::AnyNodeRef::from)
            .unwrap_or_else(|| ast::AnyNodeRef::from(import_from));
        let module = module.as_deref();

        tracing::trace!(
            "Resolving import statement from module `{}` into file `{}`",
            format_import_from_module(*level, module),
            self.file().path(db),
        );
        let module_name = ModuleName::from_import_statement(db, self.file(), import_from);

        let module_name = match module_name {
            Ok(module_name) => module_name,
            Err(ModuleNameResolutionError::InvalidSyntax) => {
                tracing::debug!("Failed to resolve import due to invalid syntax");
                // Invalid syntax diagnostics are emitted elsewhere.
                return;
            }
            Err(ModuleNameResolutionError::TooManyDots) => {
                tracing::debug!(
                    "Relative module resolution `{}` failed: too many leading dots",
                    format_import_from_module(*level, module),
                );
                self.report_unresolved_import(module_ref.range(), *level, module, None);
                return;
            }
            Err(ModuleNameResolutionError::UnknownCurrentModule) => {
                tracing::debug!(
                    "Relative module resolution `{}` failed: could not resolve file `{}` to a module \
                    (try adjusting configured search paths?)",
                    format_import_from_module(*level, module),
                    self.file().path(db)
                );
                self.report_unresolved_import(module_ref.range(), *level, module, None);
                return;
            }
        };

        if resolve_module(db, self.file(), &module_name).is_none() {
            self.report_unresolved_import(module_ref.range(), *level, module, Some(&module_name));
        }
    }

    pub(super) fn infer_import_from_definition(
        &mut self,
        import_from: &ast::StmtImportFrom,
        alias: &ast::Alias,
        definition: Definition<'db>,
    ) {
        let db = self.db();

        let Ok(module_name) = ModuleName::from_import_statement(db, self.file(), import_from)
        else {
            self.add_unknown_declaration_with_binding(alias.into(), definition);
            return;
        };

        if self
            .settings()
            .replace_imports_with_any
            .matches(&module_name)
            .is_include()
        {
            self.add_declaration_with_binding(
                alias.into(),
                definition,
                &DeclaredAndInferredType::are_the_same_type(Type::any()),
            );
            return;
        }

        let Some(module) = resolve_module(db, self.file(), &module_name) else {
            self.add_unknown_declaration_with_binding(alias.into(), definition);
            return;
        };

        let module_ty = Type::module_literal(db, self.file(), module);

        let name = if let Some(star_import) = definition.kind(db).as_star_import() {
            self.index
                .place_table(self.scope().file_scope_id(db))
                .symbol(star_import.symbol_id())
                .name()
        } else {
            &alias.name.id
        };

        // Avoid looking up attributes on a module if a module imports from itself
        // at the module-global scope, where the import definition itself is one of the
        // bindings for the symbol being looked up, which would cause a query cycle.
        //
        // In nested scopes (e.g. function bodies), the module's global-scope definitions
        // are resolved independently, so there is no cycle risk and the lookup is safe.
        let skip_self_referential_member_lookup = module_ty
            .as_module_literal()
            .is_some_and(|module| Some(self.file()) == module.module(db).file(db))
            && self.scope().file_scope_id(db).is_global();

        // Although it isn't the runtime semantics, we go to some trouble to prioritize a submodule
        // over module `__getattr__`, because that's what other type checkers do.
        let mut from_module_getattr = None;

        // First try loading the requested attribute from the module.
        if !skip_self_referential_member_lookup {
            if let PlaceAndQualifiers {
                place:
                    Place::Defined(DefinedPlace {
                        ty,
                        definedness: boundness,
                        ..
                    }),
                qualifiers,
            } = module_ty.member(db, name)
            {
                if &alias.name != "*" && boundness == Definedness::PossiblyUndefined {
                    // TODO: Consider loading _both_ the attribute and any submodule and unioning them
                    // together if the attribute exists but is possibly-unbound.
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
                    from_module_getattr = Some((ty, qualifiers));
                } else {
                    self.add_declaration_with_binding(
                        alias.into(),
                        definition,
                        &DeclaredAndInferredType::MightBeDifferent {
                            declared_ty: TypeAndQualifiers {
                                inner: ty,
                                origin: TypeOrigin::Declared,
                                qualifiers,
                                definition: None,
                            },
                            inferred_ty: ty,
                        },
                    );
                    return;
                }
            }
        }

        // Evaluate whether `X.Y` would constitute a valid submodule name,
        // given a `from X import Y` statement. If it is valid, this will be `Some()`;
        // else, it will be `None`.
        let full_submodule_name = ModuleName::new(name).map(|final_part| {
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
        if let Some(submodule_type) = full_submodule_name
            .as_ref()
            .and_then(|submodule_name| self.module_type_from_name(submodule_name))
        {
            self.add_declaration_with_binding(
                alias.into(),
                definition,
                &DeclaredAndInferredType::are_the_same_type(submodule_type),
            );
            return;
        }

        // We've checked for a submodule, so now we can go ahead and use a type from module
        // `__getattr__`.
        if let Some((ty, qualifiers)) = from_module_getattr {
            self.add_declaration_with_binding(
                alias.into(),
                definition,
                &DeclaredAndInferredType::MightBeDifferent {
                    declared_ty: TypeAndQualifiers {
                        inner: ty,
                        origin: TypeOrigin::Declared,
                        qualifiers,
                        definition: None,
                    },
                    inferred_ty: ty,
                },
            );
            return;
        }

        self.add_unknown_declaration_with_binding(alias.into(), definition);

        if &alias.name == "*" {
            return;
        }

        if self
            .settings()
            .allowed_unresolved_imports
            .matches(full_submodule_name.as_ref().unwrap_or(&module_name))
            .is_include()
        {
            return;
        }

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
                &mut diagnostic,
                &full_submodule_name,
                module,
            );
        }

        if !submodule_hint_added {
            hint_if_stdlib_attribute_exists_on_other_versions(
                db,
                diagnostic,
                module_ty,
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

        // Get this package's absolute module name by resolving `.`, and make sure it exists
        let Ok(thispackage_name) = ModuleName::package_for_file(db, self.file()) else {
            self.add_binding(import_from.into(), definition)
                .insert(self, Type::unknown());
            return;
        };

        let Some(module) = resolve_module(db, self.file(), &thispackage_name) else {
            self.add_binding(import_from.into(), definition)
                .insert(self, Type::unknown());
            return;
        };

        // We have `from whatever.thispackage.x.y ...` or `from .x.y ...`
        // and we want to extract `x` (to ultimately construct `whatever.thispackage.x`):

        // First we normalize to `whatever.thispackage.x.y`
        let Some(final_part) = ModuleName::from_identifier_parts(
            db,
            self.file(),
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
            db,
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
