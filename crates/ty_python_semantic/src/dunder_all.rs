use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use ty_python_core::{ProgramFile, semantic_index};

use self::collector::{Collector, DunderAllFacts, OrdinaryDunderAllEffects, collect_sync};
use crate::{Db, ProgramEnvironment};

pub(crate) mod collector;

pub(crate) fn dunder_all_names_ingredient(
    db: &dyn Db,
) -> &salsa::plumbing::function::IngredientImpl<DunderAllNamesConfiguration> {
    dunder_all_names::fn_ingredient_(db, db.zalsa())
}

/// Returns a set of names in the `__all__` variable for `file`, [`None`] if it is not defined or
/// if it contains invalid elements.
#[salsa::tracked(configuration = (pub(crate) DunderAllNamesConfiguration), attempt = ReturnOnly, returns(as_ref), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn dunder_all_names(db: &dyn Db, file: ProgramFile<'_>) -> Option<FxHashSet<Name>> {
    let source_file = file.file(db);
    let _span = tracing::trace_span!("dunder_all_names", file=?source_file.path(db)).entered();

    let module = parsed_module(db, file.python_file(db)).load(db);
    let effects = OrdinaryDunderAllEffects {
        db,
        env: ProgramEnvironment::from_file(file),
        file,
        index: semantic_index(db, file),
    };
    let mut collector = Collector::default();
    let names = match collect_sync(&mut collector, module.suite(), DunderAllFacts, &effects) {
        Ok(names) => names,
        Err(error) => match error {},
    };
    if collector.origin.is_some() && collector.invalid {
        tracing::debug!("Invalid `__all__` in `{}`", file.file(db).path(db));
    }
    names
}
