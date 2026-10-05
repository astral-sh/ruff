//! Semantic dependencies of module member lookup.
//!
//! The shared lookup owns submodule precedence and fallback selection. Providers must complete
//! each dependency or propagate an error before the lookup can return an ordinary member result.

use std::convert::Infallible;
use std::future::{Future, ready};

use ruff_db::files::File;

use itertools::Itertools;
use ty_module_resolver::{KnownModule, Module};
use ty_python_core::ProgramFile;

use crate::Db;
use crate::place::{PlaceAndQualifiers, imported_symbol};
use crate::types::{KnownClass, MemberLookupResult, ModuleLiteralType, ProgramEnvironment, Type};

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

pub(in crate::types) trait ModuleMemberEffects<'db>: sealed::Sealed {
    type Error;

    async fn module(
        &self,
        db: &'db dyn Db,
        module: ModuleLiteralType<'db>,
    ) -> Result<Module<'db>, Self::Error>;

    async fn module_file(
        &self,
        db: &'db dyn Db,
        module: Module<'db>,
    ) -> Result<Option<File>, Self::Error>;

    async fn known_module(
        &self,
        db: &'db dyn Db,
        module: Module<'db>,
    ) -> Result<Option<KnownModule>, Self::Error>;

    /// Reserves the shared lookup's metadata reads and name comparisons before they run.
    async fn checkpoint(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        module: ModuleLiteralType<'db>,
        name: &str,
    ) -> Result<(), Self::Error>;

    async fn module_type_member(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Result<MemberLookupResult<'db>, Self::Error>;

    /// Includes inspecting the importing file's submodule imports.
    async fn has_submodule_attribute(
        &self,
        db: &'db dyn Db,
        module: ModuleLiteralType<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;

    /// Obtains the imported file's real source identity before looking up its symbol.
    async fn source_file(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        file: File,
    ) -> Result<ProgramFile<'db>, Self::Error>;

    async fn resolve_submodule(
        &self,
        db: &'db dyn Db,
        module: ModuleLiteralType<'db>,
        name: &str,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    /// Preserves the canonical symbol reduction, re-export rules, and implicit member fallback.
    async fn imported_symbol(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        file: Option<ProgramFile<'db>>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;

    async fn module_getattr(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        module: ModuleLiteralType<'db>,
        name: &str,
    ) -> Result<MemberLookupResult<'db>, Self::Error>;
}

pub(in crate::types) struct LegacyInlineEffects;

impl sealed::Sealed for LegacyInlineEffects {}

impl<'db> ModuleMemberEffects<'db> for LegacyInlineEffects {
    type Error = Infallible;

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

    fn checkpoint(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _module: ModuleLiteralType<'db>,
        _name: &str,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Ok(()))
    }

    fn module_type_member(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> impl Future<Output = Result<MemberLookupResult<'db>, Self::Error>> {
        ready(Ok(KnownClass::ModuleType
            .to_instance(db, env)
            .member(db, env, name)
            .into()))
    }

    fn has_submodule_attribute(
        &self,
        db: &'db dyn Db,
        module: ModuleLiteralType<'db>,
        name: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(module.available_submodule_attributes(db).contains(name)))
    }

    fn resolve_submodule(
        &self,
        db: &'db dyn Db,
        module: ModuleLiteralType<'db>,
        name: &str,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        ready(Ok(module.resolve_submodule(db, name)))
    }

    fn imported_symbol(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        file: Option<ProgramFile<'db>>,
        name: &str,
    ) -> impl Future<Output = Result<PlaceAndQualifiers<'db>, Self::Error>> {
        ready(Ok(imported_symbol(db, env, file, name, None)))
    }

    fn module_getattr(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        module: ModuleLiteralType<'db>,
        name: &str,
    ) -> impl Future<Output = Result<MemberLookupResult<'db>, Self::Error>> {
        ready(Ok(module.try_module_getattr(db, env, name)))
    }
}
