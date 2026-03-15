//! Generates a self-contained HTML coverage report for `ty coverage --html`.
//!
//! All coverage data is serialized to JSON via serde and embedded as
//! `const COVERAGE_DATA = <json>;` in the output file. JavaScript renders
//! the file tree, summary table, and annotated source views on demand.
//! The design follows the dark IDE theme from `example.html`.

use std::io::Write as _;

use anyhow::Context;
use ruff_db::files::File;
use ruff_db::source::source_text;
use ruff_db::system::{SystemPath, SystemPathBuf};
use serde::Serialize;
use ty_project::ProjectDatabase;
use ty_python_semantic::coverage::{FileCoverageDetails, TypeCoverage};

/// Root object embedded as `const COVERAGE_DATA = <json>;` in the HTML.
#[derive(Serialize)]
struct ReportData<'a> {
    files: Vec<FileData<'a>>,
    show_todo: bool,
}

/// Per-file record embedded in the HTML report JSON.
///
/// `line_classes`: 0=empty, 1=precise, 2=imprecise, 3=dynamic, 4=todo.
/// `source`: raw (unescaped) line strings; JS HTML-escapes on render.
/// All stats are derived in JS from `line_classes` via `lineStats()`.
#[derive(Serialize)]
struct FileData<'a> {
    path: &'a str,
    line_classes: Vec<u8>,
    source: Vec<String>,
}

pub(crate) fn write_html_report(
    path: &SystemPath,
    per_file: &[(SystemPathBuf, File, FileCoverageDetails)],
    prefix: &SystemPath,
    db: &ProjectDatabase,
    show_todo: bool,
) -> anyhow::Result<()> {
    let mut files: Vec<FileData> = Vec::with_capacity(per_file.len());

    for (fpath, file, details) in per_file {
        let rel = fpath
            .strip_prefix(prefix)
            .unwrap_or(fpath.as_path())
            .as_str();

        let src = source_text(db, *file);
        let source: Vec<String> = src.as_str().split('\n').map(str::to_owned).collect();
        let line_count = source.len();

        let mut line_classes: Vec<u8> = Vec::with_capacity(line_count);
        for lineno in 1..=line_count {
            line_classes.push(match details.line_map.get(&lineno) {
                None => 0,
                Some(TypeCoverage::Precise) => 1,
                Some(TypeCoverage::Imprecise) => 2,
                Some(TypeCoverage::Dynamic) => 3,
                Some(TypeCoverage::Todo) => 4,
            });
        }

        files.push(FileData {
            path: rel,
            line_classes,
            source,
        });
    }

    let report = ReportData { files, show_todo };
    let json = serde_json::to_string(&report)?;
    // Escape `</` so the browser's HTML parser won't see `</script>` inside JSON.
    let json = json.replace("</", "<\\/");

    const TEMPLATE: &str = include_str!("report_template.html");
    let (before, after) = TEMPLATE
        .split_once("const COVERAGE_DATA = [];")
        .expect("report_template.html is missing the `const COVERAGE_DATA = [];` sentinel");

    let mut out = String::with_capacity(json.len() + TEMPLATE.len());
    out.push_str(before);
    out.push_str("const COVERAGE_DATA = ");
    out.push_str(&json);
    out.push_str(";");
    out.push_str(after);

    let std_path = path.as_std_path();
    if let Some(parent) = std_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory `{}`", parent.display()))?;
    }
    let mut f =
        std::fs::File::create(std_path).with_context(|| format!("Failed to create `{path}`"))?;
    f.write_all(out.as_bytes())
        .with_context(|| format!("Failed to write `{path}`"))?;

    Ok(())
}
