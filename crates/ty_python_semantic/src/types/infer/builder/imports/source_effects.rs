//! Source dependencies of plain-import and `from`-import definitions.
//!
//! Definition bodies own module selection and attribute-versus-submodule precedence. Providers
//! supply completed semantic operations; a queued provider propagates unavailable work without
//! inserting a recovery binding or publishing diagnostics from the unfinished definition.

use std::convert::Infallible;
use std::future::{Future, ready};

use ruff_db::files::File;
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};
use ty_module_resolver::{
    ImportingFile, Module, ModuleName, ModuleNameResolutionError, resolve_module,
};
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::{FileScopeId, ScopeId};

use crate::Db;
use crate::types::diagnostic::MISSING_DIRECT_DEPENDENCY;
use crate::types::infer::TypeInferenceBuilder;
use crate::types::{MemberLookupError, MemberLookupResult, ModuleLiteralType, Type};

pub(in crate::types::infer) mod sealed {
    pub(in crate::types::infer) trait Sealed {}
}

/// Local work performed by the shared import body after its dependencies complete.
#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) enum ImportFromWork {
    FullModuleName {
        bytes: usize,
    },
    TopmostParentName {
        bytes: usize,
    },
    ImportedName {
        bytes: usize,
    },
    SubmoduleName {
        module_bytes: usize,
        member_bytes: usize,
    },
    PossiblyMissingDiagnostic {
        module_bytes: usize,
        member_bytes: usize,
    },
    FinalDeclaration,
}

pub(in crate::types::infer) trait ImportFromEffects<'db>:
    sealed::Sealed
{
    type Error;

    async fn definition_kind(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Self::Error>;

    async fn file_scope_id(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> Result<FileScopeId, Self::Error>;

    async fn module_literal_file(
        &self,
        db: &'db dyn Db,
        module: ModuleLiteralType<'db>,
    ) -> Result<Option<File>, Self::Error>;

    /// Reserves the next local operation before any owned data or definition records are written.
    async fn checkpoint(&self, work: ImportFromWork) -> Result<(), Self::Error>;

    async fn log_invalid_import_syntax(&self) -> Result<(), Self::Error>;

    async fn report_unresolved_plain_import(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        alias: &ast::Alias,
        module_name: &ModuleName,
    ) -> Result<(), Self::Error>;

    async fn direct_dependency_lint_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
    ) -> Result<bool, Self::Error>;

    async fn direct_dependency_in_stub(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
    ) -> Result<bool, Self::Error>;

    /// Checks reachability, import policy and dependency metadata after the lint and stub gates.
    async fn check_direct_dependency_tail(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module: Module<'db>,
        range: TextRange,
    ) -> Result<(), Self::Error>;

    /// Resolves the import's written name using prepared file and resolver facts.
    /// The provider accounts for the identifier bytes and relative-name resolution work.
    async fn module_name(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        import_from: &ast::StmtImportFrom,
    ) -> Result<Result<ModuleName, ModuleNameResolutionError>, Self::Error>;

    /// Evaluates configured import replacement before resolving or reading the imported module.
    async fn replace_import_with_any(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module_name: &ModuleName,
    ) -> Result<bool, Self::Error>;

    async fn resolve_module(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module_name: &ModuleName,
    ) -> Result<Option<Module<'db>>, Self::Error>;

    /// Uses the prepared module kind to retain the importing file for packages.
    async fn module_literal(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module: Module<'db>,
    ) -> Result<ModuleLiteralType<'db>, Self::Error>;

    /// Includes the canonical module lookup's re-export and semantic fallback operations.
    async fn static_member(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module: ModuleLiteralType<'db>,
        name: &str,
    ) -> Result<MemberLookupResult<'db>, Self::Error>;

    async fn submodule_type(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module_name: &ModuleName,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn check_deprecated(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        range: TextRange,
        ty: Type<'db>,
    ) -> Result<(), Self::Error>;

    /// Preserves declaration checks, assignment recovery, and the final binding write.
    /// An unavailable declaration or assignment relation cannot be replaced by direct insertion.
    async fn insert_binding<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error>;

    /// Reporting a failed module `__getattr__` can require recreating its call bindings.
    async fn report_getattr_error(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        error: MemberLookupError<'db>,
        module: ModuleLiteralType<'db>,
        alias: &ast::Alias,
        name: &str,
    ) -> Result<(), Self::Error>;

    /// Completes version and source-symbol hint queries without retaining a diagnostic guard
    /// across suspension. The provider accounts for those queries and rendered-name bytes.
    async fn report_missing_import(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module: ModuleLiteralType<'db>,
        module_name: &ModuleName,
        name: &str,
        alias: &ast::Alias,
        full_submodule_name: Option<&ModuleName>,
    ) -> Result<(), Self::Error>;
}

pub(in crate::types::infer) struct LegacyInlineEffects;

impl sealed::Sealed for LegacyInlineEffects {}

impl<'db> ImportFromEffects<'db> for LegacyInlineEffects {
    type Error = Infallible;

    fn definition_kind(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<&'db DefinitionKind<'db>, Self::Error>> {
        ready(Ok(definition.kind(db)))
    }

    fn file_scope_id(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> impl Future<Output = Result<FileScopeId, Self::Error>> {
        ready(Ok(scope.file_scope_id(db)))
    }

    fn module_literal_file(
        &self,
        db: &'db dyn Db,
        module: ModuleLiteralType<'db>,
    ) -> impl Future<Output = Result<Option<File>, Self::Error>> {
        ready(Ok(module.module(db).file(db)))
    }

    fn checkpoint(&self, _work: ImportFromWork) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Ok(()))
    }

    fn log_invalid_import_syntax(&self) -> impl Future<Output = Result<(), Self::Error>> {
        tracing::debug!("Failed to resolve import due to invalid syntax");
        ready(Ok(()))
    }

    fn report_unresolved_plain_import(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        alias: &ast::Alias,
        module_name: &ModuleName,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        builder.report_unresolved_import(alias.range(), 0, Some(&alias.name), Some(module_name));
        ready(Ok(()))
    }

    fn direct_dependency_lint_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(builder
            .context
            .is_lint_enabled(&MISSING_DIRECT_DEPENDENCY)))
    }

    fn direct_dependency_in_stub(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(builder.in_stub()))
    }

    fn check_direct_dependency_tail(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module: Module<'db>,
        range: TextRange,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        builder.check_direct_dependency_tail(module, range);
        ready(Ok(()))
    }

    fn module_name(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        import_from: &ast::StmtImportFrom,
    ) -> impl Future<Output = Result<Result<ModuleName, ModuleNameResolutionError>, Self::Error>>
    {
        let db = builder.db();
        let importing_file = ImportingFile::File(
            builder.file(),
            builder.program_environment().resolver_environment(db),
        );
        ready(Ok(ModuleName::from_import_statement(
            db,
            importing_file,
            import_from,
        )))
    }

    fn replace_import_with_any(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module_name: &ModuleName,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(builder
            .settings()
            .replace_imports_with_any
            .matches(module_name)
            .is_include()))
    }

    fn resolve_module(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module_name: &ModuleName,
    ) -> impl Future<Output = Result<Option<Module<'db>>, Self::Error>> {
        let db = builder.db();
        let importing_file = ImportingFile::File(
            builder.file(),
            builder.program_environment().resolver_environment(db),
        );
        ready(Ok(resolve_module(db, importing_file, module_name)))
    }

    fn module_literal(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module: Module<'db>,
    ) -> impl Future<Output = Result<ModuleLiteralType<'db>, Self::Error>> {
        let db = builder.db();
        ready(Ok(ModuleLiteralType::new(
            db,
            module,
            module
                .kind(db)
                .is_package()
                .then_some(builder.program_file()),
        )))
    }

    fn static_member(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module: ModuleLiteralType<'db>,
        name: &str,
    ) -> impl Future<Output = Result<MemberLookupResult<'db>, Self::Error>> {
        ready(Ok(module.static_member(
            builder.db(),
            builder.program_environment(),
            name,
        )))
    }

    fn submodule_type(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module_name: &ModuleName,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        ready(Ok(builder.module_type_from_name(module_name)))
    }

    fn check_deprecated(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        range: TextRange,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        builder.check_deprecated(range, ty);
        ready(Ok(()))
    }

    fn insert_binding<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        builder
            .add_binding(alias.into(), definition)
            .insert(builder, ty);
        ready(Ok(()))
    }

    fn report_getattr_error(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        error: MemberLookupError<'db>,
        module: ModuleLiteralType<'db>,
        alias: &ast::Alias,
        name: &str,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        error.report_module_getattr_import_diagnostic(&builder.context, module, alias, name);
        ready(Ok(()))
    }

    fn report_missing_import(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        module: ModuleLiteralType<'db>,
        module_name: &ModuleName,
        name: &str,
        alias: &ast::Alias,
        full_submodule_name: Option<&ModuleName>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        builder.report_missing_import_member(module, module_name, name, alias, full_submodule_name);
        ready(Ok(()))
    }
}
