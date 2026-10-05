use std::convert::Infallible;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::predicate::StarImportPlaceholderPredicate;
use ty_python_core::{ProgramFile, Truthiness, place_table};

use crate::dunder_all::dunder_all_names;
use crate::place::{
    DefinedPlace, Definedness, Place, PlaceAndQualifiers, RequiresExplicitReExport, imported_symbol,
};
use crate::{Db, ProgramEnvironment};

pub(super) struct OrdinaryStarImportEffects<'db> {
    pub(super) db: &'db dyn Db,
}

shared_semantic_family! {
    #[synchronous(SynchronousStarImportEffects)]
    pub(crate) trait StarImportEffects<'db> {
        type Error;

        #[operation(source)]
        async fn symbol_name(&self, predicate: StarImportPlaceholderPredicate<'db>) -> Result<&'db Name, Self::Error>;
        #[operation(local)]
        async fn referenced_file(&self, predicate: StarImportPlaceholderPredicate<'db>) -> Result<ProgramFile<'db>, Self::Error>;
        #[operation(child)]
        async fn export_names(&self, file: ProgramFile<'db>) -> Result<Option<&'db FxHashSet<Name>>, Self::Error>;
        #[operation(local)]
        async fn contains_name(&self, names: &FxHashSet<Name>, name: &Name) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn excluded(&self, file: ProgramFile<'db>, name: &Name) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn imported_symbol(&self, env: &ProgramEnvironment<'db>, file: ProgramFile<'db>, name: &Name, reexport: Option<RequiresExplicitReExport>) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    }

    #[synchronous(analyze_star_import_sync)]
    #[capabilities(effects = StarImportEffects)]
    #[passive_values(RequiresExplicitReExport::No, Truthiness::AlwaysFalse, Truthiness::AlwaysTrue, Truthiness::Ambiguous)]
    pub(crate) async fn analyze_star_import_with<'db, E: StarImportEffects<'db>>(
        env: &ProgramEnvironment<'db>,
        predicate: StarImportPlaceholderPredicate<'db>,
        effects: &E,
    ) -> Result<Truthiness, E::Error> {
        let name = effects.symbol_name(predicate).await?;
        let file = effects.referenced_file(predicate).await?;
        let reexport = match effects.export_names(file).await? {
            Some(names) => {
                if effects.contains_name(names, name).await? {
                    Some(RequiresExplicitReExport::No)
                } else {
                    effects.excluded(file, name).await?;
                    return Ok(Truthiness::AlwaysFalse);
                }
            }
            None => None,
        };

        // Inclusion in `__all__` permits re-export but does not establish that the name is defined.
        let imported = effects.imported_symbol(env, file, name, reexport).await?;
        Ok(match imported.place {
            Place::Defined(DefinedPlace { definedness: Definedness::AlwaysDefined, .. }) => Truthiness::AlwaysTrue,
            Place::Defined(DefinedPlace { definedness: Definedness::PossiblyUndefined, .. }) => Truthiness::Ambiguous,
            Place::Undefined => Truthiness::AlwaysFalse,
        })
    }
}

impl<'db> SynchronousStarImportEffects<'db> for OrdinaryStarImportEffects<'db> {
    type Error = Infallible;

    fn symbol_name(
        &self,
        predicate: StarImportPlaceholderPredicate<'db>,
    ) -> Result<&'db Name, Infallible> {
        Ok(place_table(self.db, predicate.scope(self.db))
            .symbol(predicate.symbol_id(self.db))
            .name())
    }

    fn referenced_file(
        &self,
        predicate: StarImportPlaceholderPredicate<'db>,
    ) -> Result<ProgramFile<'db>, Infallible> {
        Ok(predicate.referenced_file(self.db))
    }

    fn export_names(
        &self,
        file: ProgramFile<'db>,
    ) -> Result<Option<&'db FxHashSet<Name>>, Infallible> {
        Ok(dunder_all_names(self.db, file))
    }

    fn contains_name(&self, names: &FxHashSet<Name>, name: &Name) -> Result<bool, Infallible> {
        Ok(names.contains(name))
    }

    fn excluded(&self, file: ProgramFile<'db>, name: &Name) -> Result<(), Infallible> {
        tracing::trace!(
            "Symbol `{}` (via star import) not found in `__all__` of `{}`",
            name,
            file.file(self.db).path(self.db)
        );
        Ok(())
    }

    fn imported_symbol(
        &self,
        env: &ProgramEnvironment<'db>,
        file: ProgramFile<'db>,
        name: &Name,
        reexport: Option<RequiresExplicitReExport>,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(imported_symbol(self.db, env, Some(file), name, reexport))
    }
}
