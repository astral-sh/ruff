use crate::collector::Collector;
use crate::resolver::{ModuleDb, Resolver};
pub use crate::settings::ImportMapSettings;
use anyhow::Result;
use red_knot_python_semantic::{Program, ProgramSettings, PythonVersion, SearchPathSettings};
use ruff_db::system::SystemPathBuf;
use ruff_python_ast::helpers::to_module_path;
use ruff_python_ast::PySourceType;
use ruff_python_parser::{parse, AsMode};
use ruff_source_file::Locator;
use std::path::Path;

mod collector;
mod resolver;
mod settings;

pub fn generate(
    path: &Path,
    package: Option<&Path>,
    source_type: PySourceType,
    settings: &ImportMapSettings,
) -> Result<()> {
    // Read and parse the source code.
    let source = std::fs::read_to_string(path)?;
    let parsed = parse(&source, source_type.as_mode())?;
    let locator = Locator::new(&source);
    let module_path = package.and_then(|package| to_module_path(package, path));

    // Collect the imports.
    let imports = Collector::default().collect(parsed.syntax());

    // Initialize the module database.
    let db = ModuleDb::new();
    Program::from_settings(
        &db,
        &ProgramSettings {
            target_version: PythonVersion::default(),
            search_paths: SearchPathSettings::new(SystemPathBuf::from(
                "/Users/crmarsh/workspace/ruff/scripts",
            )),
        },
    )?;

    println!("{:?}", imports);

    for import in imports {
        println!("{:?}", import);

        let resolved = Resolver::new(module_path.as_deref(), &db).resolve(&import);
        println!("{:?}", resolved);
    }

    Ok(())
}
