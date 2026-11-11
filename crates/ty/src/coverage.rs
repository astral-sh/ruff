//! Writes the HTML coverage report for `ty check --html-report`.
//!
//! The report follows the layout of mypy's `--html-report`: an `index.html` page with one row per
//! file, and one page per file that shows its source with each line colored by its
//! [`LineCoverage`].

use std::fmt::Write as _;

use anyhow::Context;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use ruff_db::files::File;
use ruff_db::source::source_text;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ty_project::{Db as _, ProjectDatabase};
use ty_python_semantic::coverage::{LineCoverage, line_coverage};

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

    /// Writes a bar that shows the share of precise, imprecise, and dynamic lines. The rest of
    /// the bar, the share of empty lines, shows the bar's background.
    #[expect(clippy::cast_precision_loss)]
    fn write_bar(self, out: &mut String, class: &str) {
        let total = self.total().max(1) as f64;
        let _ = write!(out, "<div class=\"{class}\">");
        for (count, segment) in [
            (self.precise, "precise"),
            (self.imprecise, "imprecise"),
            (self.dynamic, "dynamic"),
        ] {
            let width = count as f64 / total * 100.0;
            let _ = write!(
                out,
                "<span class=\"{segment}\" style=\"width: {width:.2}%\"></span>"
            );
        }
        out.push_str("</div>");
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
    let mut files: Vec<(File, SystemPathBuf)> = db
        .project()
        .files(db)
        .iter()
        .filter_map(|file| Some((file, file.path(db).as_system_path()?.to_path_buf())))
        .collect();
    files.sort_by(|(_, a), (_, b)| a.cmp(b));

    // Show paths relative to the deepest directory that contains all files.
    let prefix = common_parent(files.iter().map(|(_, path)| path.as_path()));
    let relative_path =
        |path: &SystemPathBuf| path.strip_prefix(&prefix).unwrap_or(path).to_path_buf();

    // Write the file pages in parallel, keeping only their line counts so that the coverage of all
    // files is never in memory at once. Salsa databases can't be shared between threads, so each
    // Rayon job gets its own handle.
    let counts: Vec<CoverageCounts> = files
        .par_iter()
        .map_with(db.clone(), |db, (file, path)| {
            let relative_path = relative_path(path);
            let (page, counts) = file_page(db, *file, &relative_path);
            write_file(&output_dir.join(page_path(&relative_path)), &page)?;
            Ok(counts)
        })
        .collect::<anyhow::Result<_>>()?;

    let mut rows = String::new();
    let mut total = CoverageCounts::default();

    for ((_, path), counts) in files.iter().zip(counts) {
        let relative_path = relative_path(path);
        total.merge(counts);
        write_summary_row(&mut rows, &relative_path, counts);
    }

    let mut project = String::new();
    escape_html(&mut project, db.project().name(db));

    let mut total_bar = String::new();
    total.write_bar(&mut total_bar, "bar total");
    let summary = format!(
        "{:.2}% of {} lines in {} files are imprecise or dynamic",
        total.imprecision() * 100.0,
        total.total(),
        files.len()
    );

    let index = format!(
        "<!DOCTYPE html>
<html lang=\"en\">
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>{project} type coverage</title>
<style>{STYLE}</style>
</head>
<body>
<h1>{project} type coverage</h1>
<p class=\"subtitle\">{summary}</p>
{total_bar}
<div class=\"table-wrapper\">
<table class=\"summary\">
<thead><tr><th>File</th><th></th><th>Imprecision</th><th>Lines</th><th>Precise</th><th>Imprecise</th><th>Dynamic</th><th>Empty</th></tr></thead>
<tbody>
{rows}</tbody>
</table>
</div>
</body>
</html>
"
    );
    write_file(&output_dir.join("index.html"), &index)?;

    Ok(total)
}

/// The path of a file's page, relative to the report directory.
fn page_path(relative_path: &SystemPath) -> String {
    format!("files/{relative_path}.html")
}

/// Renders the page for a single file and counts its lines.
fn file_page(
    db: &ProjectDatabase,
    file: File,
    relative_path: &SystemPath,
) -> (String, CoverageCounts) {
    let source = source_text(db, file);
    let lines = line_coverage(db, file);
    let mut counts = CoverageCounts::default();
    let mut code = String::new();

    for (index, line) in source.as_str().lines().enumerate() {
        let (coverage, notes) = lines
            .get(index)
            .map_or((LineCoverage::Empty, &[][..]), |line| {
                (line.coverage, &line.notes[..])
            });
        counts.add(coverage);

        let number = index + 1;
        let class = match coverage {
            LineCoverage::Empty => "empty",
            LineCoverage::Precise => "precise",
            LineCoverage::Imprecise => "imprecise",
            LineCoverage::Dynamic => "dynamic",
        };
        let _ = write!(code, "<div class=\"line {class}\" id=\"L{number}\"");
        if !notes.is_empty() {
            code.push_str(" title=\"");
            escape_html(&mut code, &notes.join("\n"));
            code.push('"');
        }
        let _ = write!(code, "><a href=\"#L{number}\">{number}</a><code>");
        escape_html(&mut code, line);
        code.push_str("</code></div>\n");
    }

    // Link back to `index.html` from the page's directory.
    let index_link = "../".repeat(page_path(relative_path).matches('/').count()) + "index.html";
    let mut title = String::new();
    escape_html(&mut title, relative_path.as_str());
    let summary = format!(
        "{} lines, {:.2}% imprecise or dynamic",
        counts.total(),
        counts.imprecision() * 100.0
    );

    let page = format!(
        "<!DOCTYPE html>
<html lang=\"en\">
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>{title}</title>
<style>{STYLE}</style>
</head>
<body>
<header>
<a href=\"{index_link}\">All files</a>
<h1>{title}</h1>
<span class=\"subtitle\">{summary}</span>
<span class=\"legend\"><span class=\"precise\">precise</span><span class=\"imprecise\">imprecise</span><span class=\"dynamic\">dynamic</span></span>
</header>
<div class=\"source\"><div class=\"lines\">
{code}</div></div>
</body>
</html>
"
    );

    (page, counts)
}

fn write_summary_row(out: &mut String, relative_path: &SystemPath, counts: CoverageCounts) {
    out.push_str("<tr><td><a href=\"");
    out.push_str(&page_path(relative_path));
    out.push_str("\">");
    escape_html(out, relative_path.as_str());
    out.push_str("</a></td><td>");
    counts.write_bar(out, "bar");
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

/// The report's stylesheet, which is copied into every page so that each page works on its own.
///
/// Like mypy's report, precise lines are green, imprecise lines are yellow, and dynamic lines are
/// a warning color. Unlike mypy, we leave empty lines (comments, docstrings, and blank lines)
/// uncolored, so that green only marks lines whose types ty checked and found precise.
const STYLE: &str = include_str!("coverage/report.css");
