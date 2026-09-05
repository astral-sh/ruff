use ty_python_core::definition::Definition;
use ty_python_core::use_def_map;

use crate::Db;
use crate::place::definitions::DefinitionResolution;
use crate::types::may_exist_at_runtime;

mod collection;
mod fixtures;

pub use fixtures::{
    FixtureBinding, FixtureExposure, FixtureNameSource, fixture_bindings_for_parameter,
    fixture_exposures_for_definition, pytest_global_plugin_files,
};

/// Returns whether `definition` remains bound in its defining scope and may exist at runtime.
fn is_available_definition<'db>(db: &'db dyn Db, definition: Definition<'db>) -> bool {
    let resolution = DefinitionResolution::from_bindings(
        db,
        use_def_map(db, definition.scope(db)).end_of_scope_bindings(definition.place(db)),
    );
    resolution.definitions().contains(&definition) && may_exist_at_runtime(db, definition)
}
