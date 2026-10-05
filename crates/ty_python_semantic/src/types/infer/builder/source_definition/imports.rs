//! Import dependencies supplied by the owning definition session.

use std::future::{Future, ready};

use ruff_db::files::File;
use ruff_python_ast as ast;
use ruff_text_size::TextRange;
use ty_module_resolver::{
    ImportingFile, KnownModule, Module, ModuleName, ModuleNameResolutionError, resolve_module,
};
use ty_python_core::ProgramFile;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::{FileScopeId, ScopeId};

use super::{QueuedDefinitionEffects, SourceDefinitionEffect, unsupported};
use crate::place::PlaceAndQualifiers;
use crate::types::callable::scheduled_probe::Boundary;
use crate::types::diagnostic::MISSING_DIRECT_DEPENDENCY;
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::imports::source_effects::{
    self, ImportFromEffects, ImportFromWork,
};
use crate::types::module_member_effects::{self, ModuleMemberEffects};
use crate::types::{MemberLookupError, MemberLookupResult, ModuleLiteralType, Type};
use crate::{Db, ProgramEnvironment};

impl source_effects::sealed::Sealed for QueuedDefinitionEffects<'_, '_, '_> {}

impl<'db> ImportFromEffects<'db> for QueuedDefinitionEffects<'_, 'db, '_> {
    type Error = Boundary;

    async fn definition_kind(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Self::Error> {
        self.work(1).await?;
        Ok(definition.kind(db))
    }

    async fn file_scope_id(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> Result<FileScopeId, Self::Error> {
        self.work(1).await?;
        Ok(scope.file_scope_id(db))
    }

    async fn module_literal_file(
        &self,
        db: &'db dyn Db,
        module: ModuleLiteralType<'db>,
    ) -> Result<Option<File>, Self::Error> {
        self.work(2).await?;
        Ok(module.module(db).file(db))
    }

    async fn checkpoint(&self, work: ImportFromWork) -> Result<(), Self::Error> {
        let units = match work {
            ImportFromWork::FullModuleName { bytes } => {
                bytes.checked_mul(5).and_then(|bytes| bytes.checked_add(4))
            }
            ImportFromWork::TopmostParentName { bytes } => {
                bytes.checked_mul(7).and_then(|bytes| bytes.checked_add(4))
            }
            ImportFromWork::ImportedName { bytes } => bytes.checked_add(1),
            ImportFromWork::SubmoduleName {
                module_bytes,
                member_bytes,
            } => module_bytes
                .checked_add(member_bytes)
                .and_then(|bytes| bytes.checked_add(2)),
            ImportFromWork::PossiblyMissingDiagnostic { .. } => {
                return Err(unsupported(SourceDefinitionEffect::ImportDiagnostic));
            }
            ImportFromWork::FinalDeclaration => Some(1),
        };
        self.work(units.ok_or(Boundary::CostOverflow)?).await
    }

    async fn log_invalid_import_syntax(&self) -> Result<(), Self::Error> {
        self.work(1).await?;
        tracing::debug!("Failed to resolve import due to invalid syntax");
        Ok(())
    }

    fn report_unresolved_plain_import(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _alias: &ast::Alias,
        _module_name: &ModuleName,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ImportDiagnostic)))
    }

    async fn direct_dependency_lint_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
    ) -> Result<bool, Self::Error> {
        self.work(1).await?;
        Ok(builder.context.is_lint_enabled(&MISSING_DIRECT_DEPENDENCY))
    }

    async fn direct_dependency_in_stub(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
    ) -> Result<bool, Self::Error> {
        self.work(1).await?;
        let path_len = builder.file().path(builder.db()).as_str().len();
        self.work(path_len.checked_add(3).ok_or(Boundary::CostOverflow)?)
            .await?;
        Ok(builder.in_stub())
    }

    fn check_direct_dependency_tail(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _module: Module<'db>,
        _range: TextRange,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ImportPolicy)))
    }

    async fn module_name(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        import: &ast::StmtImportFrom,
    ) -> Result<Result<ModuleName, ModuleNameResolutionError>, Self::Error> {
        let bytes = import.module.as_ref().map_or(0, |module| module.id.len());
        self.work(bytes.checked_add(2).ok_or(Boundary::CostOverflow)?)
            .await?;
        let db = builder.db();
        let importing = ImportingFile::File(
            builder.file(),
            builder.program_environment().resolver_environment(db),
        );
        Ok(ModuleName::from_import_statement(db, importing, import))
    }

    async fn replace_import_with_any(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        _module_name: &ModuleName,
    ) -> Result<bool, Self::Error> {
        self.work(1).await?;
        if builder.settings().replace_imports_with_any.is_empty() {
            Ok(false)
        } else {
            Err(unsupported(SourceDefinitionEffect::ImportPolicy))
        }
    }

    async fn resolve_module(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        name: &ModuleName,
    ) -> Result<Option<Module<'db>>, Self::Error> {
        self.work(
            name.as_str()
                .len()
                .checked_add(1)
                .ok_or(Boundary::CostOverflow)?,
        )
        .await?;
        let db = builder.db();
        let importing = ImportingFile::File(
            builder.file(),
            builder.program_environment().resolver_environment(db),
        );
        Ok(resolve_module(db, importing, name))
    }

    async fn module_literal(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module: Module<'db>,
    ) -> Result<ModuleLiteralType<'db>, Self::Error> {
        self.work(2).await?;
        let db = builder.db();
        Ok(ModuleLiteralType::new(
            db,
            module,
            module
                .kind(db)
                .is_package()
                .then_some(builder.program_file()),
        ))
    }

    async fn static_member(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module: ModuleLiteralType<'db>,
        name: &str,
    ) -> Result<MemberLookupResult<'db>, Self::Error> {
        module
            .static_member_with(builder.db(), builder.program_environment(), self, name)
            .await
    }

    fn submodule_type(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _name: &ModuleName,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::Submodule)))
    }

    async fn check_deprecated(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        range: TextRange,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        builder.check_deprecated_with(self, range, ty).await
    }

    async fn insert_binding<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        builder
            .add_binding_with(self, alias.into(), definition)
            .await?
            .insert_with(builder, self, ty)
            .await?;
        Ok(())
    }

    fn report_getattr_error(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _error: MemberLookupError<'db>,
        _module: ModuleLiteralType<'db>,
        _alias: &ast::Alias,
        _name: &str,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ImportDiagnostic)))
    }

    fn report_missing_import(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _module: ModuleLiteralType<'db>,
        _module_name: &ModuleName,
        _name: &str,
        _alias: &ast::Alias,
        _full_submodule_name: Option<&ModuleName>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ImportDiagnostic)))
    }
}

impl module_member_effects::sealed::Sealed for QueuedDefinitionEffects<'_, '_, '_> {}

impl<'db> ModuleMemberEffects<'db> for QueuedDefinitionEffects<'_, 'db, '_> {
    type Error = Boundary;

    fn module(
        &self,
        db: &'db dyn Db,
        module: ModuleLiteralType<'db>,
    ) -> impl Future<Output = Result<Module<'db>, Self::Error>> {
        ready(Ok(module.module(db)))
    }

    fn module_file(
        &self,
        db: &'db dyn Db,
        module: Module<'db>,
    ) -> impl Future<Output = Result<Option<File>, Self::Error>> {
        ready(Ok(module.file(db)))
    }

    fn known_module(
        &self,
        db: &'db dyn Db,
        module: Module<'db>,
    ) -> impl Future<Output = Result<Option<KnownModule>, Self::Error>> {
        ready(Ok(module.known(db)))
    }

    fn source_file(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        file: File,
    ) -> impl Future<Output = Result<ProgramFile<'db>, Self::Error>> {
        ready(Ok(ProgramFile::new(db, file, env.program(db))))
    }

    async fn checkpoint(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _module: ModuleLiteralType<'db>,
        name: &str,
    ) -> Result<(), Self::Error> {
        self.work(name.len().checked_add(8).ok_or(Boundary::CostOverflow)?)
            .await
    }

    fn module_type_member(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _name: &str,
    ) -> impl Future<Output = Result<MemberLookupResult<'db>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ModuleTypeMember)))
    }

    async fn has_submodule_attribute(
        &self,
        db: &'db dyn Db,
        module: ModuleLiteralType<'db>,
        _name: &str,
    ) -> Result<bool, Self::Error> {
        self.work(1).await?;
        if module.importing_file(db).is_none() {
            Ok(false)
        } else {
            Err(unsupported(SourceDefinitionEffect::Submodule))
        }
    }

    fn resolve_submodule(
        &self,
        _db: &'db dyn Db,
        _module: ModuleLiteralType<'db>,
        _name: &str,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::Submodule)))
    }

    async fn imported_symbol(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        file: Option<ProgramFile<'db>>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.imported_symbol(db, env, file, name, None).await
    }

    fn module_getattr(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _module: ModuleLiteralType<'db>,
        _name: &str,
    ) -> impl Future<Output = Result<MemberLookupResult<'db>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ModuleGetattr)))
    }
}
