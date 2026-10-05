//! Import inference retains real source identities while awaiting imported definitions.

use ruff_db::files::File;
use ruff_python_ast as ast;
use ruff_text_size::TextRange;
use salsa::execution_probe::{RunError, RunResult};
use ty_module_resolver::{KnownModule, Module, ModuleName, ModuleNameResolutionError};
use ty_python_core::ProgramFile;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::{FileScopeId, ScopeId};
use ty_python_core::symbol::ScopedSymbolId;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::PlaceAndQualifiers;
use crate::place::source_effects::reachability_with;
use crate::types::diagnostic::MISSING_DIRECT_DEPENDENCY;
use crate::types::infer::builder::imports::source_effects::{
    self, ImportFromEffects, ImportFromWork,
};
use crate::types::infer::builder::imports::statement::{
    ImportStatementEffects, module_matches_imported_child,
};
use crate::types::infer::builder::source_binding::SourceBindingEffects;
use crate::types::infer::{DefinitionInference, TypeInferenceBuilder};
use crate::types::module_member_effects::{self, ModuleMemberEffects};
use crate::types::{MemberLookupError, MemberLookupResult, ModuleLiteralType, Type};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn import_module_name(
        &self,
        file: ProgramFile<'db>,
        import: &ast::StmtImportFrom,
    ) -> RunResult<Result<ModuleName, ModuleNameResolutionError>> {
        self.check_file_program(file).await?;
        let tail = import.module.as_deref();
        let tail_len = tail.map_or(0, str::len);
        if import.level == 0 {
            let work = Self::checked(tail_len.checked_mul(5).and_then(|n| n.checked_add(4)))?;
            let bytes = Self::checked(tail_len.checked_add(4 * size_of::<ModuleName>()))?;
            return self
                .local(work, bytes, || {
                    tail.and_then(ModuleName::new)
                        .ok_or(ModuleNameResolutionError::InvalidSyntax)
                })
                .await;
        }

        let Some(module) = self.access.file_module(file).await? else {
            return Ok(Err(ModuleNameResolutionError::UnknownCurrentModule));
        };
        let kind = module.kind_with(self.access.endpoint()).await?;
        let name = module.name_with(self.access.endpoint()).await?;
        let length = Self::checked(name.as_str().len().checked_add(tail_len))?;
        let work = Self::checked(length.checked_mul(8).and_then(|n| n.checked_add(16)))?;
        // The parent name, optional tail and extended result can each allocate. Admit their
        // construction and disposal before retaining the resulting name across resolution.
        let bytes = Self::checked(
            length
                .checked_add(4 * size_of::<ModuleName>())
                .and_then(|n| n.checked_mul(3)),
        )?;
        self.local(work, bytes, || {
            let level = import.level - u32::from(kind.is_package());
            let mut parent = name.as_str();
            for _ in 0..level {
                let Some((next, _)) = parent.rsplit_once('.') else {
                    return Err(ModuleNameResolutionError::TooManyDots);
                };
                parent = next;
            }
            let Some(mut absolute) = ModuleName::new(parent) else {
                return Err(ModuleNameResolutionError::InvalidSyntax);
            };
            if let Some(tail) = tail {
                let tail = ModuleName::new(tail).ok_or(ModuleNameResolutionError::InvalidSyntax)?;
                absolute.extend(&tail);
            }
            Ok(absolute)
        })
        .await
    }

    pub(in crate::types::infer::builder) async fn infer_import_from_statement_source(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        import: &ast::StmtImportFrom,
    ) -> RunResult<()> {
        self.allocate_future(|| builder.infer_import_from_statement_with(self, import))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ImportStatementEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn log_module_resolution(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _import: &ast::StmtImportFrom,
    ) -> RunResult<()> {
        self.work(1).await
    }

    async fn log_module_name_error(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _import: &ast::StmtImportFrom,
        _error: &ModuleNameResolutionError,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ImportDiagnostic).await
    }

    async fn report_unresolved_from_import(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _import: &ast::StmtImportFrom,
        _module_name: Option<&ModuleName>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ImportDiagnostic).await
    }

    async fn next_alias<'ast>(
        &self,
        import: &'ast ast::StmtImportFrom,
        cursor: &mut usize,
    ) -> RunResult<Option<&'ast ast::Alias>> {
        self.local(2, 0, || {
            let next = import.names.get(*cursor);
            *cursor += usize::from(next.is_some());
            next
        })
        .await
    }

    async fn definitions(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        alias: &ast::Alias,
    ) -> RunResult<&'db [Definition<'db>]> {
        let work = self
            .local(1, 0, || builder.index.definition_lookup_work())
            .await?;
        self.local(work, 0, || builder.index.definitions(alias))
            .await
    }

    async fn next_definition(
        &self,
        definitions: &[Definition<'db>],
        cursor: &mut usize,
    ) -> RunResult<Option<Definition<'db>>> {
        self.local(2, 0, || {
            let next = definitions.get(*cursor).copied();
            *cursor += usize::from(next.is_some());
            next
        })
        .await
    }

    async fn star_import_is_unreachable(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        symbol: ScopedSymbolId,
    ) -> RunResult<bool> {
        let scope = ImportFromEffects::file_scope_id(self, builder.db(), builder.scope()).await?;
        let (use_def, mut bindings) = self
            .local(4, 0, || {
                let use_def = builder.index.use_def_map(scope);
                (use_def, use_def.reachable_symbol_bindings(symbol))
            })
            .await?;
        while let Some(binding) = self.local(4, 0, || bindings.next()).await? {
            if binding
                .binding
                .is_defined_and(|candidate| candidate == definition)
            {
                let cache = SourceBindingEffects::reachability_cache(self, builder).await?;
                return Ok(reachability_with(
                    self,
                    Some(cache),
                    use_def.reachability_constraints(),
                    use_def.predicates(),
                    binding.reachability_constraint,
                )
                .await?
                .is_always_false());
            }
        }
        Ok(false)
    }

    async fn definition(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionInference<'db>> {
        self.access.definition(definition).await
    }

    async fn bindings<'inference>(
        &self,
        inference: &'inference DefinitionInference<'db>,
        definition: Definition<'db>,
    ) -> RunResult<impl ExactSizeIterator<Item = (Definition<'db>, Type<'db>)> + 'inference> {
        self.local(2, 0, || inference.bindings(definition)).await
    }

    async fn next_binding(
        &self,
        bindings: &mut impl ExactSizeIterator<Item = (Definition<'db>, Type<'db>)>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(2, 0, || bindings.next().map(|(_, ty)| ty)).await
    }

    async fn module_literal_module(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        literal: ModuleLiteralType<'db>,
    ) -> RunResult<Module<'db>> {
        ModuleMemberEffects::module(self, builder.db(), literal).await
    }

    async fn module_matches_imported_child(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        parent: Module<'db>,
        child: Module<'db>,
        alias: &ast::Alias,
    ) -> RunResult<bool> {
        let child_name = child.name_with(self.access.endpoint()).await?;
        let parent_name = parent.name_with(self.access.endpoint()).await?;
        let work = Self::checked(
            child_name
                .as_str()
                .len()
                .checked_add(parent_name.as_str().len())
                .and_then(|len| len.checked_add(alias.name.as_str().len()))
                .and_then(|len| len.checked_mul(5))
                .and_then(|len| len.checked_add(8)),
        )?;
        // Selecting the parent can copy its name; admit both the copy and its disposal.
        let bytes = Self::checked(
            child_name
                .as_str()
                .len()
                .checked_add(4 * size_of::<ModuleName>()),
        )?;
        self.local(work, bytes, || {
            module_matches_imported_child(parent_name, child_name, alias.name.as_str())
        })
        .await
    }

    async fn extend_definition(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
        inference: &DefinitionInference<'db>,
    ) -> RunResult<()> {
        self.merge_definition(builder, definition, inference).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> source_effects::sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ImportFromEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn definition_kind(
        &self,
        _db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionKind<'db>> {
        self.field(
            definition
                .read_fields(self.access.endpoint().field_request_context())
                .kind(),
        )
        .await
    }

    async fn file_scope_id(&self, _db: &'db dyn Db, scope: ScopeId<'db>) -> RunResult<FileScopeId> {
        self.field(
            scope
                .read_fields(self.access.endpoint().field_request_context())
                .file_scope_id(),
        )
        .await
    }

    async fn module_literal_file(
        &self,
        db: &'db dyn Db,
        module: ModuleLiteralType<'db>,
    ) -> RunResult<Option<File>> {
        let module = ModuleMemberEffects::module(self, db, module).await?;
        ModuleMemberEffects::module_file(self, db, module).await
    }

    async fn checkpoint(&self, work: ImportFromWork) -> RunResult<()> {
        match work {
            ImportFromWork::FullModuleName { bytes }
            | ImportFromWork::TopmostParentName { bytes } => {
                let scans = match work {
                    ImportFromWork::FullModuleName { .. } => 5,
                    _ => 7,
                };
                // Parsing scans components and validates identifiers before copying the name.
                // Parent selection also scans for a dot and the first component. Admit disposal
                // with creation so an interrupted definition can drop either owned name.
                let units = Self::checked(bytes.checked_mul(scans).and_then(|n| n.checked_add(4)))?;
                // Compact strings can allocate more than the text length for short names.
                let requested_bytes =
                    Self::checked(bytes.checked_add(4 * size_of::<ModuleName>()))?;
                self.local(units, requested_bytes, || ()).await
            }
            ImportFromWork::ImportedName { bytes } => {
                self.work(Self::checked(bytes.checked_add(3))?).await
            }
            ImportFromWork::SubmoduleName { .. } => {
                self.unavailable(SourceOperation::Submodule).await
            }
            ImportFromWork::PossiblyMissingDiagnostic { .. } => {
                self.unavailable(SourceOperation::ImportDiagnostic).await
            }
            ImportFromWork::FinalDeclaration => {
                self.unavailable(SourceOperation::ImportBinding).await
            }
        }
    }

    async fn log_invalid_import_syntax(&self) -> RunResult<()> {
        self.unavailable(SourceOperation::ImportDiagnostic).await
    }

    async fn report_unresolved_plain_import(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _alias: &ast::Alias,
        _module_name: &ModuleName,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ImportDiagnostic).await
    }

    async fn direct_dependency_lint_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
    ) -> RunResult<bool> {
        self.is_lint_enabled_source(builder, &MISSING_DIRECT_DEPENDENCY)
            .await
    }

    async fn direct_dependency_in_stub(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
    ) -> RunResult<bool> {
        self.check_file_program(builder.program_file()).await?;
        self.file_is_stub(builder.file()).await
    }

    async fn check_direct_dependency_tail(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _module: Module<'db>,
        _range: TextRange,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ImportPolicy).await
    }

    async fn module_name(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        import: &ast::StmtImportFrom,
    ) -> RunResult<Result<ModuleName, ModuleNameResolutionError>> {
        self.import_module_name(builder.program_file(), import)
            .await
    }

    async fn replace_import_with_any(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        _module_name: &ModuleName,
    ) -> RunResult<bool> {
        let settings = self.access.analysis_settings(builder.file()).await?;
        let unchanged = self
            .local(1, 0, || settings.replace_imports_with_any.is_empty())
            .await?;
        if unchanged {
            Ok(false)
        } else {
            self.unavailable(SourceOperation::ImportPolicy).await
        }
    }

    async fn resolve_module(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        name: &ModuleName,
    ) -> RunResult<Option<Module<'db>>> {
        let program = self
            .environment_program(builder.program_environment())
            .await?;
        self.access
            .resolve_module(program, name, Some(builder.file()))
            .await
    }

    async fn module_literal(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module: Module<'db>,
    ) -> RunResult<ModuleLiteralType<'db>> {
        let kind = module.kind_with(self.access.endpoint()).await?;
        let importing = self
            .local(2, 0, || kind.is_package().then_some(builder.program_file()))
            .await?;
        self.access.module_literal(module, importing).await
    }

    async fn static_member(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module: ModuleLiteralType<'db>,
        name: &str,
    ) -> RunResult<MemberLookupResult<'db>> {
        module
            .static_member_with(builder.db(), builder.program_environment(), self, name)
            .await
    }

    async fn submodule_type(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _name: &ModuleName,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::Submodule).await
    }
    async fn check_deprecated(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        range: TextRange,
        ty: Type<'db>,
    ) -> RunResult<()> {
        builder.check_deprecated_with(self, range, ty).await
    }
    async fn insert_binding<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        builder
            .add_binding_with(self, alias.into(), definition)
            .await?
            .insert_with(builder, self, ty)
            .await?;
        Ok(())
    }
    async fn report_getattr_error(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _error: MemberLookupError<'db>,
        _module: ModuleLiteralType<'db>,
        _alias: &ast::Alias,
        _name: &str,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ImportDiagnostic).await
    }
    async fn report_missing_import(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _module: ModuleLiteralType<'db>,
        _module_name: &ModuleName,
        _name: &str,
        _alias: &ast::Alias,
        _full_submodule_name: Option<&ModuleName>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ImportDiagnostic).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> module_member_effects::sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ModuleMemberEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn module(
        &self,
        _db: &'db dyn Db,
        module: ModuleLiteralType<'db>,
    ) -> RunResult<Module<'db>> {
        self.field(
            module
                .field_requests(self.access.endpoint().field_request_context())
                .module(),
        )
        .await
    }

    async fn module_file(&self, _db: &'db dyn Db, module: Module<'db>) -> RunResult<Option<File>> {
        module.file_with(self.access.endpoint()).await
    }

    async fn known_module(
        &self,
        _db: &'db dyn Db,
        module: Module<'db>,
    ) -> RunResult<Option<KnownModule>> {
        module.known_with(self.access.endpoint()).await
    }

    async fn checkpoint(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _module: ModuleLiteralType<'db>,
        name: &str,
    ) -> RunResult<()> {
        self.work(Self::checked(name.len().checked_add(8))?).await
    }
    async fn module_type_member(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _name: &str,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.unavailable(SourceOperation::ModuleTypeMember).await
    }
    async fn has_submodule_attribute(
        &self,
        db: &'db dyn Db,
        module: ModuleLiteralType<'db>,
        name: &str,
    ) -> RunResult<bool> {
        let fields = self.access.endpoint().field_request_context();
        #[cfg(debug_assertions)]
        {
            let importing_file = self
                .field(module.field_requests(fields)._importing_file())
                .await?;
            let resolved = self.module(db, module).await?;
            let kind = resolved.kind_with(self.access.endpoint()).await?;
            self.local(1, 0, || {
                debug_assert_eq!(importing_file.is_some(), kind.is_package());
            })
            .await?;
        }
        let importing_file = self
            .field(module.field_requests(fields)._importing_file())
            .await?;
        let importing_file = self.local(4, 0, || importing_file).await?;
        let Some(importing_file) = importing_file else {
            return Ok(false);
        };
        let prepared = self.access.prepare_existing(importing_file).await?;
        let parent = self.module(db, module).await?;
        let parent_name = parent.name_with(self.access.endpoint()).await?;
        let parent_len = self.local(3, 0, || parent_name.as_str().len()).await?;
        let mut imports = self
            .local(1, 0, || prepared.index.imported_modules())
            .await?;
        while let Some(imported) = self.local(1, 0, || imports.next()).await? {
            let imported_len = self.local(1, 0, || imported.as_str().len()).await?;
            let work = Self::checked(
                imported_len
                    .checked_add(parent_len)
                    .and_then(|len| len.checked_add(name.len()))
                    .and_then(|len| len.checked_mul(5))
                    .and_then(|len| len.checked_add(8)),
            )?;
            // Relative names and their first components may each allocate. Include their
            // disposal so cancellation after this step does not leave uncharged cleanup.
            let bytes = Self::checked(
                imported_len
                    .checked_mul(2)
                    .and_then(|len| len.checked_add(4 * size_of::<ModuleName>())),
            )?;
            let parent = self.module(db, module).await?;
            let parent_name = parent.name_with(self.access.endpoint()).await?;
            if self
                .local(work, bytes, || {
                    ModuleLiteralType::imported_submodule_attribute_from_name(parent_name, imported)
                        .is_some_and(|attribute| attribute == name)
                })
                .await?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    async fn source_file(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        file: File,
    ) -> RunResult<ProgramFile<'db>> {
        let program = self.environment_program(env).await?;
        let prepared = self.access.prepare_file(file, program).await?;
        if self.physical_file(prepared.file).await? != file
            || self
                .field(
                    prepared
                        .file
                        .read_fields(self.access.endpoint().field_request_context())
                        .program(),
                )
                .await?
                != program
        {
            return Err(RunError::Contract("prepared imported file is foreign"));
        }
        Ok(prepared.file)
    }
    async fn resolve_submodule(
        &self,
        _db: &'db dyn Db,
        _module: ModuleLiteralType<'db>,
        _name: &str,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::Submodule).await
    }
    async fn imported_symbol(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        file: Option<ProgramFile<'db>>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.imported_symbol(db, env, file, name, None).await
    }
    async fn module_getattr(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _module: ModuleLiteralType<'db>,
        _name: &str,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.unavailable(SourceOperation::ModuleGetattr).await
    }
}
