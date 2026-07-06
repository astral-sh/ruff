//! ERB view field-set extractor — the presentation-tier harvest.
//!
//! # Doctrine (fuzzy-recipe-codebook.md §8c — "detected config becomes data")
//!
//! An ERB view is a **detected configuration artifact**: it names, via
//! `<receiver>.<field>` references, exactly which model fields a route
//! projects to the user. Per the config-as-data rule, that artifact
//! becomes a **data input to the codebook** — the referenced field SET —
//! never code to transcribe. We do NOT parse ERB/HTML structure, walk
//! Ruby expressions inside `<%= … %>`, or reproduce layout/markup. The
//! only fact recorded is *presence*: does this view, anywhere, reference
//! `<model>.<field>`? Two views projecting the same ten fields in
//! different table layouts are identical for this purpose.
//!
//! # Tier: Inferred, by construction
//!
//! This is a closed-vocabulary line scanner (no Ruby/ERB parser), the
//! same style as [`crate::functions`]'s body walk. A reference is only
//! recorded when BOTH the receiver identifier and the field identifier
//! match caller-supplied closed vocabularies (`ViewTarget::receivers` /
//! `ViewTarget::fields`) — this bounds false positives at the cost of
//! requiring the harvest stratum (schema + declarations) to already know
//! the field list. It is Inferred, not Authoritative: a helper call like
//! `format_date(issue.start_date)` and a genuine attribute read
//! `issue.start_date` are indistinguishable here (both project the
//! field, which is all this stratum claims).
//!
//! # What is NOT captured (by design, not oversight)
//!
//! - **Presentation** — HTML structure, CSS classes, i18n strings,
//!   conditionals, loops. Only the field-name SET, per the doctrine
//!   above.
//! - **Multi-hop chains** (`issue.project.name`) — only the first hop
//!   off a registered receiver is read; `project.name` is a second,
//!   independent reference the caller registers under its own
//!   `ViewTarget` if it wants it captured.
//! - **Whitespace between receiver and dot** (`issue .subject`) — Rails
//!   view code does not write this; treating it as out of scope keeps
//!   the scanner a single closed-vocab pass over the line.
//!
//! # The honest coverage denominator (`ViewFieldSet::referenced`)
//!
//! A `coverage = |known| / |referenced|` metric needs the RAW distinct
//! `<receiver>.<ident>` references as its denominator, not just the
//! subset that happens to already be in the harvested field vocabulary
//! — otherwise coverage is trivially `1.0` (every hit counted is, by
//! construction, a known hit). [`ViewFieldSet::referenced`] is that raw
//! set: every distinct identifier seen immediately after a *registered*
//! receiver + `.`, regardless of vocabulary membership.
//! [`ViewFieldSet::fields`] stays the known subset — `fields ⊆
//! referenced` always holds (enforced by construction: a candidate is
//! recorded into `referenced` unconditionally, then additionally into
//! `fields` when it matches the vocabulary).
//!
//! Note: `?` and `!` are not in the ident charset (see [`is_ident_char`]),
//! so a Ruby predicate/bang method like `issue.persisted?` is captured as
//! bare `persisted` in `referenced` — this is intentional, not a bug: the
//! scanner has no notion of "method" vs "field", only identifier text.

use std::fs;
use std::path::{Path, PathBuf};

/// One target model whose field references a view scan should look for.
///
/// `receivers` is the closed vocabulary of local-variable / instance-
/// variable names a view might bind the resource to (e.g. `["issue",
/// "@issue"]` — Rails conventionally exposes both the block-local and
/// the controller-set ivar). `fields` is the closed vocabulary of known
/// field names for `model` (typically the harvested schema + attribute
/// stratum for that model).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewTarget {
    /// Model name as harvested (e.g. `"Issue"`).
    pub model: String,
    /// Receiver identifiers a view might bind the resource to.
    pub receivers: Vec<String>,
    /// Known field names for `model` — the closed vocabulary a
    /// `<receiver>.<name>` reference must match to count.
    pub fields: Vec<String>,
}

/// One view's model-field projection: which harvested fields of
/// `resource` the ERB template references. Presence-only (§8c doctrine):
/// the SET, never the presentation. Inferred tier by nature (regex-style
/// field-reference scan, no Ruby template parse).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewFieldSet {
    /// Model name as harvested (e.g. `"Issue"`).
    pub resource: String,
    /// View file path relative to the views root (e.g.
    /// `"issues/show.html.erb"`).
    pub view: String,
    /// Referenced field names — deduped, sorted. Closed-vocab: ONLY
    /// names in the harvested field list count (a helper arg like
    /// `format_date(issue.start_date)` still matches `issue.start_date`).
    pub fields: Vec<String>,
    /// Every distinct identifier referenced immediately after a
    /// *registered* receiver + `.` — deduped, sorted — REGARDLESS of
    /// whether the identifier is in the harvested field vocabulary. The
    /// honest denominator for a `coverage = |fields| / |referenced|`
    /// metric (see the module doc). `fields` is always a subset of this
    /// set.
    pub referenced: Vec<String>,
}

/// Conservation-ledger totals for a view scan (same discipline as
/// [`crate::schema::SchemaReport`] — nothing drops silently).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewScanReport {
    /// Every `*.erb` file found under the views root.
    pub erb_files: usize,
    /// Files that produced at least one non-empty [`ViewFieldSet`] — a
    /// known field hit OR a raw `referenced` ident off a registered
    /// receiver.
    pub views_with_hits: usize,
}

/// Scan `<views_root>` for `*.erb` files and extract, per view file and
/// per target model, the set of known model fields referenced. Thin
/// wrapper over [`extract_view_field_sets_with_report`] for callers that
/// don't need the scan ledger.
#[must_use]
pub fn extract_view_field_sets(views_root: &Path, targets: &[ViewTarget]) -> Vec<ViewFieldSet> {
    extract_view_field_sets_with_report(views_root, targets).0
}

/// Like [`extract_view_field_sets`], but also returns a [`ViewScanReport`]
/// ledger of how many `*.erb` files were seen and how many produced a hit.
#[must_use]
pub fn extract_view_field_sets_with_report(
    views_root: &Path,
    targets: &[ViewTarget],
) -> (Vec<ViewFieldSet>, ViewScanReport) {
    let mut report = ViewScanReport::default();
    let mut files = Vec::new();
    collect_erb_files(views_root, &mut files);
    report.erb_files = files.len();

    let mut results = Vec::new();
    for path in &files {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let view = relative_view_path(views_root, path);
        let mut file_had_hit = false;
        for target in targets {
            let (fields, referenced) = referenced_fields(&content, target);
            if fields.is_empty() && referenced.is_empty() {
                continue;
            }
            debug_assert!(
                fields.iter().all(|f| referenced.contains(f)),
                "fields must be a subset of referenced: fields={fields:?} referenced={referenced:?}"
            );
            file_had_hit = true;
            results.push(ViewFieldSet {
                resource: target.model.clone(),
                view: view.clone(),
                fields,
                referenced,
            });
        }
        if file_had_hit {
            report.views_with_hits += 1;
        }
    }

    results.sort_by(|a, b| {
        a.view
            .cmp(&b.view)
            .then_with(|| a.resource.cmp(&b.resource))
    });
    (results, report)
}

/// Walk `dir` recursively, appending every file whose extension is
/// exactly `erb` (`*.html.erb`, `*.pdf.erb`, bare `*.erb`, …). Entries
/// are sorted before recursing so the result is deterministic —
/// [`crate::schema::parse_tables_dir`]'s discipline.
fn collect_erb_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_erb_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("erb") {
            out.push(path);
        }
    }
}

/// `path` relative to `root`, rendered with `/` separators regardless of
/// platform (the view path is a stable identifier, not a filesystem
/// path to reopen).
fn relative_view_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// The field references in `content` for one [`ViewTarget`]: every
/// `<receiver>.<ident>` where `receiver` is one of `target.receivers`.
/// Returns `(fields, referenced)` — `fields` is the closed-vocab subset
/// (only `<ident>` values in `target.fields`), `referenced` is every
/// distinct `<ident>` seen regardless of vocabulary membership. Both
/// deduped + sorted; `fields` is always a subset of `referenced`.
fn referenced_fields(content: &str, target: &ViewTarget) -> (Vec<String>, Vec<String>) {
    let mut found = std::collections::BTreeSet::new();
    let mut referenced = std::collections::BTreeSet::new();
    for line in content.lines() {
        for receiver in &target.receivers {
            scan_line_for_receiver(line, receiver, &target.fields, &mut found, &mut referenced);
        }
    }
    (
        found.into_iter().collect(),
        referenced.into_iter().collect(),
    )
}

/// Scan one `line` for occurrences of `receiver` immediately followed by
/// `.<identifier>`. Every such `<identifier>` is recorded into
/// `referenced` unconditionally; it is ADDITIONALLY recorded into `found`
/// when it matches one of `fields` exactly (so `found ⊆ referenced` by
/// construction). `receiver` must sit on a word boundary (the preceding
/// character, if any, must not itself be an identifier character) — this
/// rejects `reissue.subject` as a match for receiver `issue`, and lets
/// `issue` / `@issue` coexist as distinct receivers without `issue`
/// falsely matching inside `@issue`.
fn scan_line_for_receiver(
    line: &str,
    receiver: &str,
    fields: &[String],
    found: &mut std::collections::BTreeSet<String>,
    referenced: &mut std::collections::BTreeSet<String>,
) {
    if receiver.is_empty() {
        return;
    }
    let chars: Vec<char> = line.chars().collect();
    let recv: Vec<char> = receiver.chars().collect();
    if chars.len() < recv.len() {
        return;
    }
    for start in 0..=(chars.len() - recv.len()) {
        if chars[start..start + recv.len()] != recv[..] {
            continue;
        }
        if start > 0 && is_ident_char(chars[start - 1]) {
            continue;
        }
        let end = start + recv.len();
        if end >= chars.len() || chars[end] != '.' {
            continue;
        }
        let field_start = end + 1;
        let mut field_end = field_start;
        while field_end < chars.len() && is_ident_char(chars[field_end]) {
            field_end += 1;
        }
        if field_end == field_start {
            continue;
        }
        let candidate: String = chars[field_start..field_end].iter().collect();
        referenced.insert(candidate.clone());
        if fields.iter().any(|f| f == &candidate) {
            found.insert(candidate);
        }
    }
}

/// Identifier-forming characters for the word-boundary check. `@` is
/// included so a bare receiver (`issue`) cannot match inside an ivar
/// token (`@issue`) that starts one character earlier — the two are
/// registered as separate receivers when both are wanted.
fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '@'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_view(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn issue_target() -> ViewTarget {
        ViewTarget {
            model: "Issue".to_string(),
            receivers: vec!["issue".to_string(), "@issue".to_string()],
            fields: vec![
                "subject".to_string(),
                "status_id".to_string(),
                "start_date".to_string(),
            ],
        }
    }

    fn scratch_dir(case: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("ruff_ruby_spo_views_{}_{case}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }

    /// (a) A plain `receiver.field` reference is captured.
    #[test]
    fn simple_field_reference_is_captured() {
        let root = scratch_dir("simple");
        write_view(&root, "issues/show.html.erb", "<%= issue.subject %>\n");

        let sets = extract_view_field_sets(&root, &[issue_target()]);
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].resource, "Issue");
        assert_eq!(sets[0].view, "issues/show.html.erb");
        assert_eq!(sets[0].fields, vec!["subject".to_string()]);
        assert_eq!(sets[0].referenced, vec!["subject".to_string()]);

        let _ = fs::remove_dir_all(&root);
    }

    /// (b) The ivar form (`@issue.field`) is captured as its own
    /// registered receiver, without colliding with the bare `issue` form.
    #[test]
    fn ivar_receiver_is_captured() {
        let root = scratch_dir("ivar");
        write_view(&root, "issues/show.html.erb", "<%= @issue.subject %>\n");

        let sets = extract_view_field_sets(&root, &[issue_target()]);
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].fields, vec!["subject".to_string()]);
        assert_eq!(sets[0].referenced, vec!["subject".to_string()]);

        let _ = fs::remove_dir_all(&root);
    }

    /// (c) A field reference wrapped in a helper call is still captured —
    /// this stratum only records presence, not the surrounding expression.
    #[test]
    fn helper_wrapped_reference_is_captured() {
        let root = scratch_dir("helper");
        write_view(
            &root,
            "issues/show.html.erb",
            "<%= format_date(issue.start_date) %>\n",
        );

        let sets = extract_view_field_sets(&root, &[issue_target()]);
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].fields, vec!["start_date".to_string()]);
        assert_eq!(sets[0].referenced, vec!["start_date".to_string()]);

        let _ = fs::remove_dir_all(&root);
    }

    /// (d) A field NOT in the closed vocabulary must not land in `fields`
    /// — but it IS captured in `referenced`, the raw honest-denominator
    /// set (§ module doc): a registered-receiver reference is recorded
    /// regardless of vocabulary membership.
    #[test]
    fn unknown_field_is_not_captured() {
        let root = scratch_dir("unknown_field");
        write_view(&root, "issues/show.html.erb", "<%= issue.frobnicate %>\n");

        let sets = extract_view_field_sets(&root, &[issue_target()]);
        assert_eq!(sets.len(), 1, "referenced-only hit must still be reported");
        assert!(
            sets[0].fields.is_empty(),
            "unknown field must not be captured as a field: {:?}",
            sets[0].fields
        );
        assert_eq!(sets[0].referenced, vec!["frobnicate".to_string()]);

        let _ = fs::remove_dir_all(&root);
    }

    /// (e) A receiver NOT registered on the target must not be captured,
    /// even though its field name is in the closed vocabulary.
    #[test]
    fn unknown_receiver_is_not_captured() {
        let root = scratch_dir("unknown_receiver");
        write_view(&root, "issues/show.html.erb", "<%= other.subject %>\n");

        let sets = extract_view_field_sets(&root, &[issue_target()]);
        assert!(
            sets.is_empty(),
            "unregistered receiver must not be captured: {sets:?}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// (f) Word-boundary: `reissue` must not match receiver `issue`.
    #[test]
    fn word_boundary_rejects_receiver_substring() {
        let root = scratch_dir("word_boundary");
        write_view(&root, "issues/show.html.erb", "<%= reissue.subject %>\n");

        let sets = extract_view_field_sets(&root, &[issue_target()]);
        assert!(
            sets.is_empty(),
            "`reissue` must not match receiver `issue`: {sets:?}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// The scan ledger counts every `*.erb` file (non-`.erb` files like
    /// `.slim` are excluded) and how many produced at least one hit.
    #[test]
    fn report_counts_erb_files_and_views_with_hits() {
        let root = scratch_dir("report");
        write_view(&root, "issues/show.html.erb", "<%= issue.subject %>\n");
        write_view(&root, "issues/index.html.erb", "<p>no fields here</p>\n");
        write_view(&root, "layouts/base.html.slim", "not an erb file\n");

        let (sets, report) = extract_view_field_sets_with_report(&root, &[issue_target()]);
        assert_eq!(sets.len(), 1);
        assert_eq!(report.erb_files, 2, "only *.erb files count, slim excluded");
        assert_eq!(report.views_with_hits, 1);

        let _ = fs::remove_dir_all(&root);
    }

    /// Multiple targets against one file: each non-empty projection
    /// yields its own `ViewFieldSet`, sorted by view then model.
    #[test]
    fn multiple_targets_each_yield_a_view_field_set() {
        let root = scratch_dir("multi_target");
        write_view(
            &root,
            "issues/show.html.erb",
            "<%= issue.subject %> assigned to <%= user.name %>\n",
        );
        let user_target = ViewTarget {
            model: "User".to_string(),
            receivers: vec!["user".to_string()],
            fields: vec!["name".to_string()],
        };

        let sets = extract_view_field_sets(&root, &[issue_target(), user_target]);
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].resource, "Issue");
        assert_eq!(sets[0].fields, vec!["subject".to_string()]);
        assert_eq!(sets[0].referenced, vec!["subject".to_string()]);
        assert_eq!(sets[1].resource, "User");
        assert_eq!(sets[1].fields, vec!["name".to_string()]);
        assert_eq!(sets[1].referenced, vec!["name".to_string()]);

        let _ = fs::remove_dir_all(&root);
    }

    // ─────── Part 1: raw-ident census (`referenced`) tests ───────

    /// (a) An unknown ident lands in `referenced` but not in `fields`,
    /// while a known ident referenced alongside it lands in both.
    #[test]
    fn unknown_ident_lands_in_referenced_not_in_fields() {
        let root = scratch_dir("referenced_denominator");
        write_view(
            &root,
            "issues/show.html.erb",
            "<%= issue.subject %> <%= issue.frobnicate %>\n",
        );

        let sets = extract_view_field_sets(&root, &[issue_target()]);
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].fields, vec!["subject".to_string()]);
        assert_eq!(
            sets[0].referenced,
            vec!["frobnicate".to_string(), "subject".to_string()]
        );
        assert!(!sets[0].fields.contains(&"frobnicate".to_string()));

        let _ = fs::remove_dir_all(&root);
    }

    /// (b) `fields ⊆ referenced` holds across a realistic mix of known and
    /// unknown idents referenced off both a bare and an ivar receiver.
    #[test]
    fn fields_is_always_a_subset_of_referenced() {
        let root = scratch_dir("subset_invariant");
        write_view(
            &root,
            "issues/show.html.erb",
            "<%= issue.subject %> <%= issue.made_up_helper %> <%= @issue.status_id %>\n",
        );

        let sets = extract_view_field_sets(&root, &[issue_target()]);
        assert_eq!(sets.len(), 1);
        assert!(
            sets[0].fields.len() < sets[0].referenced.len(),
            "fixture must contain at least one unknown ident: fields={:?} referenced={:?}",
            sets[0].fields,
            sets[0].referenced
        );
        for field in &sets[0].fields {
            assert!(
                sets[0].referenced.contains(field),
                "fields must be a subset of referenced: field {field:?} missing from {:?}",
                sets[0].referenced
            );
        }

        let _ = fs::remove_dir_all(&root);
    }
}
