use std::path::{Path, PathBuf};

use anyhow::Result;
use ruff_db::diagnostic::Diagnostic;
use ty_project::ProjectDatabase;

use model::Documentation;
use render::write_site;

#[derive(Debug)]
pub struct GenerationOptions {
    pub document_private_items: bool,
    pub default_selection: bool,
    pub generator_version: String,
}

#[derive(Debug)]
pub struct GenerationResult {
    pub documented_files: usize,
    pub index_path: PathBuf,
    pub project_name: String,
    pub warnings: Vec<Diagnostic>,
}

pub fn generate(
    db: &ProjectDatabase,
    output_dir: &Path,
    options: GenerationOptions,
) -> Result<GenerationResult> {
    let documentation = Documentation::collect(
        db,
        options.document_private_items,
        options.default_selection,
        options.generator_version,
    );
    let index_path = write_site(&documentation, output_dir)?;

    Ok(GenerationResult {
        documented_files: documentation.documented_files,
        index_path,
        project_name: documentation.project_name,
        warnings: documentation.warnings,
    })
}

mod collect;
mod model;
mod render;
mod syntax;
