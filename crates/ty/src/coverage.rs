//! Writes the HTML coverage report for `ty check --html-report`.
//!
//! The report follows the layout of mypy's `--html-report`: an `index.html` page with one row per
//! file, and one page per file that shows its source with each line colored by its
//! [`LineCoverage`].

use std::fmt::Write as _;

use anyhow::Context;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use ruff_db::files::File;
use ruff_db::source::source_text;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ty_project::{Db as _, ProjectDatabase};
use ty_python_semantic::coverage::{LineCoverage, line_coverage};

/// The coverage of a single project file.
struct FileCoverage {
    path: SystemPathBuf,
    file: File,
    /// The classification of each line, indexed by zero-based line number.
    lines: Vec<LineCoverage>,
}

/// Line counts for a file, or for all files in the report.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct CoverageCounts {
    empty: usize,
    precise: usize,
    imprecise: usize,
    dynamic: usize,
}

impl CoverageCounts {
    fn add(&mut self, coverage: LineCoverage) {
        match coverage {
            LineCoverage::Empty => self.empty += 1,
            LineCoverage::Precise => self.precise += 1,
            LineCoverage::Imprecise => self.imprecise += 1,
            LineCoverage::Dynamic => self.dynamic += 1,
        }
    }

    fn merge(&mut self, other: CoverageCounts) {
        self.empty += other.empty;
        self.precise += other.precise;
        self.imprecise += other.imprecise;
        self.dynamic += other.dynamic;
    }

    fn total(self) -> usize {
        self.empty + self.precise + self.imprecise + self.dynamic
    }

    /// The share of lines that are imprecise or dynamic, as in mypy's report.
    #[expect(clippy::cast_precision_loss)]
    pub(crate) fn imprecision(self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            (self.imprecise + self.dynamic) as f64 / total as f64
        }
    }

    /// The CSS class that colors a summary row, using mypy's thresholds.
    fn quality_class(self) -> &'static str {
        let imprecision = self.imprecision();
        if imprecision == 0.0 {
            "quality-good"
        } else if imprecision < 0.2 {
            "quality-fair"
        } else {
            "quality-poor"
        }
    }
}

/// Writes the report for all project files into `output_dir`, and returns the line counts over
/// all files.
///
/// This runs after `ty check` has inferred the types of all project files, so it mostly reads
/// cached results.
pub(crate) fn write_html_report(
    db: &ProjectDatabase,
    output_dir: &SystemPath,
) -> anyhow::Result<CoverageCounts> {
    let files: Vec<File> = db.project().files(db).iter().collect();

    // Salsa databases can't be shared between threads, so each Rayon job gets its own handle.
    let mut files: Vec<FileCoverage> = files
        .into_par_iter()
        .map_with(db.clone(), |db, file| {
            let path = file.path(db).as_system_path()?.to_path_buf();
            let lines = line_coverage(db, file);
            Some(FileCoverage { path, file, lines })
        })
        .flatten()
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));

    // Show paths relative to the deepest directory that contains all files.
    let prefix = common_parent(files.iter().map(|file| file.path.as_path()));

    let mut rows = String::new();
    let mut total = CoverageCounts::default();

    for file in &files {
        let relative_path = file.path.strip_prefix(&prefix).unwrap_or(&file.path);
        let page_path = format!("files/{relative_path}.html");
        let (page, counts) = file_page(db, file, relative_path, &page_path);
        write_file(&output_dir.join(&page_path), &page)?;

        total.merge(counts);
        write_summary_row(&mut rows, relative_path.as_str(), Some(&page_path), counts);
    }

    let mut footer = String::new();
    write_summary_row(&mut footer, "Total", None, total);

    let index = format!(
        "<!DOCTYPE html>
<html lang=\"en\">
<head>
<meta charset=\"utf-8\">
<title>ty coverage report</title>
<style>{STYLE}</style>
</head>
<body>
<h1>ty coverage report</h1>
<table class=\"summary\">
<thead><tr><th>File</th><th>Imprecision</th><th>Lines</th><th>Precise</th><th>Imprecise</th><th>Dynamic</th><th>Empty</th></tr></thead>
<tbody>
{rows}</tbody>
<tfoot>
{footer}</tfoot>
</table>
</body>
</html>
"
    );
    write_file(&output_dir.join("index.html"), &index)?;

    Ok(total)
}

/// Renders the page for a single file and counts its lines.
fn file_page(
    db: &ProjectDatabase,
    file: &FileCoverage,
    relative_path: &SystemPath,
    page_path: &str,
) -> (String, CoverageCounts) {
    let source = source_text(db, file.file);
    let mut counts = CoverageCounts::default();
    let mut line_numbers = String::new();
    let mut code = String::new();

    for (index, line) in source.as_str().lines().enumerate() {
        let coverage = file.lines.get(index).copied().unwrap_or_default();
        counts.add(coverage);

        let number = index + 1;
        let _ = writeln!(
            line_numbers,
            "<a id=\"L{number}\" href=\"#L{number}\">{number}</a>"
        );
        let class = match coverage {
            LineCoverage::Empty => "empty",
            LineCoverage::Precise => "precise",
            LineCoverage::Imprecise => "imprecise",
            LineCoverage::Dynamic => "dynamic",
        };
        let _ = write!(code, "<span class=\"line-{class}\">");
        escape_html(&mut code, line);
        code.push_str("</span>\n");
    }

    // Link back to `index.html` from the page's directory.
    let index_link = "../".repeat(page_path.matches('/').count()) + "index.html";
    let mut title = String::new();
    escape_html(&mut title, relative_path.as_str());

    let page = format!(
        "<!DOCTYPE html>
<html lang=\"en\">
<head>
<meta charset=\"utf-8\">
<title>{title}</title>
<style>{STYLE}</style>
</head>
<body>
<p><a href=\"{index_link}\">Index</a></p>
<h2>{title}</h2>
<p class=\"legend\"><span class=\"line-precise\">precise</span> <span class=\"line-imprecise\">imprecise</span> <span class=\"line-dynamic\">dynamic</span></p>
<table class=\"source\"><tr><td class=\"lineno\"><pre>{line_numbers}</pre></td><td><pre>{code}</pre></td></tr></table>
</body>
</html>
"
    );

    (page, counts)
}

fn write_summary_row(out: &mut String, name: &str, link: Option<&str>, counts: CoverageCounts) {
    let _ = write!(
        out,
        "<tr class=\"{}\"><td class=\"name\">",
        counts.quality_class()
    );
    match link {
        Some(link) => {
            let _ = write!(out, "<a href=\"{link}\">");
            escape_html(out, name);
            out.push_str("</a>");
        }
        None => escape_html(out, name),
    }
    let _ = writeln!(
        out,
        "</td><td>{:.2}%</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
        counts.imprecision() * 100.0,
        counts.total(),
        counts.precise,
        counts.imprecise,
        counts.dynamic,
        counts.empty,
    );
}

fn escape_html(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

fn write_file(path: &SystemPath, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory `{parent}`"))?;
    }
    std::fs::write(path, contents).with_context(|| format!("Failed to write `{path}`"))
}

/// Returns the deepest directory that contains all `paths`.
fn common_parent<'a>(mut paths: impl Iterator<Item = &'a SystemPath>) -> SystemPathBuf {
    let Some(mut prefix) = paths.next().and_then(SystemPath::parent) else {
        return SystemPathBuf::new();
    };

    for path in paths {
        while !path.starts_with(prefix) {
            let Some(parent) = prefix.parent() else {
                return SystemPathBuf::new();
            };
            prefix = parent;
        }
    }

    prefix.to_path_buf()
}

/// The colors match mypy's report: empty and precise lines are green, imprecise lines are yellow,
/// and dynamic lines are red.
const STYLE: &str = "
body { font-family: sans-serif; margin: 2em; }
table.summary { border-collapse: collapse; }
table.summary th, table.summary td { border: 1px solid #aaa; padding: 0.3em 0.6em; text-align: right; }
table.summary .name, table.summary th:first-child { text-align: left; }
table.summary a { text-decoration: none; }
.quality-good { background-color: #dfd; }
.quality-fair { background-color: #ffa; }
.quality-poor { background-color: #faa; }
table.source { border-collapse: collapse; }
table.source td { padding: 0; vertical-align: top; }
td.lineno pre { text-align: right; padding-right: 0.5em; }
td.lineno a { color: #999; text-decoration: none; }
pre { margin: 0; }
pre span { display: inline-block; min-width: 100%; }
.line-empty, .line-precise { background-color: #dfd; }
.line-imprecise { background-color: #ffa; }
.line-dynamic { background-color: #faa; }
.legend span { padding: 0.1em 0.5em; }
";
