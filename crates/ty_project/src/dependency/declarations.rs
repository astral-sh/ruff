use std::collections::BTreeMap;

use compact_str::CompactString;
use pep508_rs::Requirement;
use ruff_db::files::File;
use ruff_db::source::source_text;
use ruff_python_ast::script::ScriptSourceMap;
use ruff_text_size::{TextRange, TextSize};
use serde::Deserialize;
use toml::Spanned;
use ty_python_semantic::dependency::DependencyProjectKind;

use crate::Db;
use crate::script::script_tag;

/// A dependency declaration that can be checked without evaluating environment markers.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub(super) struct DependencyDeclaration {
    pub(super) name: CompactString,
    pub(super) range: TextRange,
}

/// Returns unconditional runtime and optional dependency declarations with their source locations.
///
/// Invalid metadata is not evidence that a dependency is unused. Dependency groups and build
/// requirements are excluded because they commonly provide tools rather than imported modules.
pub(super) fn declarations(
    db: &dyn Db,
    file: File,
    kind: DependencyProjectKind,
) -> Option<&[DependencyDeclaration]> {
    match kind {
        DependencyProjectKind::Project => project_declarations(db, file),
        DependencyProjectKind::Script => script_declarations(db, file),
    }
}

#[salsa::tracked(returns(as_deref), heap_size=ruff_memory_usage::heap_size)]
fn project_declarations(db: &dyn Db, file: File) -> Option<Box<[DependencyDeclaration]>> {
    let source = source_text(db, file);
    if source.read_error().is_some() {
        return None;
    }

    parse_project(source.as_str())
}

fn parse_project(source: &str) -> Option<Box<[DependencyDeclaration]>> {
    let metadata: ProjectMetadata = toml::from_str(source).ok()?;
    let project = metadata.project.unwrap_or_default();
    let requirements = project
        .dependencies
        .into_iter()
        .chain(project.optional_dependencies.into_values().flatten());
    parse_requirements(requirements, None)
}

#[salsa::tracked(returns(as_deref), heap_size=ruff_memory_usage::heap_size)]
fn script_declarations(db: &dyn Db, file: File) -> Option<Box<[DependencyDeclaration]>> {
    let tag = script_tag(db, file)?;
    let metadata: ScriptMetadata = toml::from_str(tag.metadata()).ok()?;
    parse_requirements(metadata.dependencies, Some(tag.source_map()))
}

fn parse_requirements(
    requirements: impl IntoIterator<Item = Spanned<String>>,
    source_map: Option<&ScriptSourceMap>,
) -> Option<Box<[DependencyDeclaration]>> {
    let mut declarations = Vec::new();
    for requirement in requirements {
        let parsed: Requirement = requirement.get_ref().parse().ok()?;
        // The dependency graph covers every supported environment. An installed distribution can
        // satisfy another declaration even when this declaration's marker does not apply.
        if !parsed.marker.is_true() {
            continue;
        }

        let span = requirement.span();
        let range = TextRange::new(
            TextSize::try_from(span.start).ok()?,
            TextSize::try_from(span.end).ok()?,
        );
        declarations.push(DependencyDeclaration {
            name: CompactString::new(parsed.name.as_ref()),
            range: source_map.map_or(range, |source_map| source_map.map_range(range)),
        });
    }

    declarations.sort_unstable_by_key(|declaration| declaration.range.start());
    Some(declarations.into_boxed_slice())
}

#[derive(Deserialize)]
struct ProjectMetadata {
    project: Option<ProjectDependencies>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ProjectDependencies {
    #[serde(default)]
    dependencies: Vec<Spanned<String>>,
    #[serde(default)]
    optional_dependencies: BTreeMap<String, Vec<Spanned<String>>>,
}

#[derive(Deserialize)]
struct ScriptMetadata {
    #[serde(default)]
    dependencies: Vec<Spanned<String>>,
}

#[cfg(test)]
mod tests {
    use anyhow::Context;
    use ruff_python_ast::script::ScriptTag;

    use super::{ScriptMetadata, parse_project, parse_requirements};

    #[test]
    fn script_declaration_ranges() -> anyhow::Result<()> {
        let source = "# café\r\n# /// script\r\n# dependencies = [\r\n#     'Some_Package',\r\n#     \"other-package>=2\",\r\n# ]\r\n# ///\r\n";
        let tag = ScriptTag::parse(source.as_bytes()).context("expected valid script metadata")?;
        let metadata: ScriptMetadata = toml::from_str(tag.metadata())?;
        let declarations = parse_requirements(metadata.dependencies, Some(tag.source_map()))
            .context("expected valid requirements")?;
        assert_eq!(
            declarations
                .iter()
                .map(|declaration| (declaration.name.as_str(), &source[declaration.range]))
                .collect::<Vec<_>>(),
            [
                ("some-package", "'Some_Package'"),
                ("other-package", r#""other-package>=2""#),
            ]
        );
        Ok(())
    }

    #[test]
    fn invalid_metadata_is_not_checked() {
        for source in [
            "[project",
            "[project]\ndependencies = ['valid', 'invalid requirement']",
        ] {
            assert!(parse_project(source).is_none(), "{source}");
        }
    }
}
