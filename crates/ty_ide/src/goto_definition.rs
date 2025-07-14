use crate::goto::find_goto_target;
use crate::stub_mapping::StubMapper;
use crate::{Db, NavigationTargets, RangedValue};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextSize};

/// Navigate to the definition of a symbol.
///
/// A "definition" is the actual implementation of a symbol, potentially in a source file
/// rather than a stub file. This differs from "declaration" which may navigate to stub files.
/// When possible, this function will map from stub file declarations to their corresponding
/// source file implementations using the `StubMapper`.
pub fn goto_definition(
    db: &dyn Db,
    file: File,
    offset: TextSize,
) -> Option<RangedValue<NavigationTargets>> {
    let module = parsed_module(db, file).load(db);
    let goto_target = find_goto_target(&module, offset)?;

    // Create a StubMapper to map from stub files to source files
    let stub_mapper = StubMapper::new(db);

    let definition_targets = goto_target.get_definition_targets(file, db, Some(&stub_mapper))?;

    Some(RangedValue {
        range: FileRange::new(file, goto_target.range()),
        value: definition_targets,
    })
}
