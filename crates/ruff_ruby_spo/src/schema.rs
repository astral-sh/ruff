//! **D-AR-3.5** — the schema stratum: physical DB columns from the Rails
//! migration DSL.
//!
//! `OpenProject` ships no `db/schema.rb` / `db/structure.sql`; the squashed
//! baseline lives in `db/migrate/tables/*.rb` — one `Tables::X <
//! Tables::Base` class per table, whose `self.table(migration)` body is a
//! plain `create_table … do |t| … end` block of `t.<type> :name, opts`
//! calls. That DSL is a fixed, enumerable vocabulary (22 distinct `t.*`
//! methods across `OpenProject`'s 99 baseline files), so a line scanner in
//! the style of [`crate::functions`] extracts it without a Ruby runtime.
//!
//! # Why this stratum matters
//!
//! The `WorkPackage` oracle diff (op-nexgen `RESIDUAL-THREE-BUCKETS.md` §4c)
//! measured that **~90% of a hand-written Rust model struct derives from
//! the column stratum alone** (name + type + nullability), and the
//! remaining typings come from validation triples the expander already
//! ships. The class-body extraction ([`crate::extract_app_with`]) reads
//! the *method/DSL* stratum; this module supplies the missing *column*
//! stratum. Columns land as [`Field`]s (`field_type` = the DSL method
//! name verbatim, `not_null` from `null: false`), so they flow through
//! the existing `field_type` / `column_not_null` predicates with no new
//! IR shape.
//!
//! # Scope (recorded honestly, conservation-ledger style)
//!
//! - **Baseline only**: incremental migrations (`db/migrate/*.rb`,
//!   `modules/*/db/migrate/*.rb`) that `add_column`/`rename_column` after
//!   the squash are NOT replayed. [`SchemaReport::columns_from`] says so.
//! - Join tables and other tables with no matching AR class are counted in
//!   [`SchemaReport::unmatched_tables`], never silently dropped.
//! - `t.index` / `t.foreign_key` / `t.check_constraint` /
//!   `t.exclusion_constraint` lines are constraint/index facts, not
//!   columns — skipped here (a later slice can lift them).

use std::fs;
use std::path::Path;

use ruff_spo_triplet::{Field, Model, ModelGraph};

/// Conservation-ledger seed for the schema pass: what was seen, what
/// matched, what didn't — nothing drops silently.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaReport {
    /// Baseline table files successfully parsed.
    pub tables_seen: usize,
    /// Tables whose inflected model name matched a model in the graph
    /// (columns merged into that model's `fields`).
    pub tables_matched: usize,
    /// Tables with no matching model (join tables, unported domains) —
    /// named, not just counted.
    pub unmatched_tables: Vec<String>,
    /// Files under `db/migrate/tables/` that could not be read or contained
    /// no recognisable `create_table` block (e.g. `base.rb`, the abstract
    /// helper) — named, not just counted.
    pub files_skipped: Vec<String>,
    /// Provenance marker: which migration surface produced the columns.
    /// Currently always `"baseline-only"` (no incremental replay).
    pub columns_from: &'static str,
}

/// One parsed baseline table: the physical columns of `table_name`,
/// plus the Rails-inflected model name they attach to.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TableColumns {
    /// The physical table name — the file stem (`work_packages`), which is
    /// exactly what `Tables::Base.table_name` derives via
    /// `name.demodulize.underscore`.
    pub(crate) table_name: String,
    /// The Rails-conventional model name (`WorkPackage`) — `PascalCase`
    /// singular of the table name.
    pub(crate) model_name: String,
    /// The columns, in declaration order, as IR fields (`field_type` =
    /// DSL method name verbatim; `not_null` from `null: false`).
    pub(crate) fields: Vec<Field>,
}

/// The `t.<method>` names that declare a typed column directly. The DSL
/// method name doubles as the emitted `field_type` token (surface label —
/// consumers own the SQL/SurrealQL mapping).
const COLUMN_TYPES: &[&str] = &[
    "string",
    "text",
    "integer",
    "bigint",
    "boolean",
    "datetime",
    "date",
    "float",
    "decimal",
    "jsonb",
    "json",
    "uuid",
    "interval",
    "tsvector",
    "tstzrange",
    "binary",
    "timestamp",
];

/// Extract a Rails app **including the schema stratum**: everything
/// [`crate::extract_app_with`] harvests, plus the baseline DB columns
/// merged into each model's `fields`, plus a [`SchemaReport`] ledger.
///
/// Column fields are appended to the matching model's `fields` (matched by
/// Rails inflection of the table name; existing same-name fields, if any
/// future pass creates them, are not duplicated). Tables with no matching
/// model are recorded in the report — the join-table population is real
/// and expected (`changesets_work_packages` et al. have no AR class).
///
/// After the column merge, a **compute-linkage pass** ([`link_computed_fields`])
/// runs over every model: a `def compute_<x>` whose class ALSO has a
/// (schema-merged) field named `<x>` gets `field.emitted_by = Some("compute_<x>")`
/// — the Rails-side equivalent of Odoo's declared `compute=`. This only ever
/// runs here (the schema-aware path), because the model-only stratum
/// ([`crate::extract_fields`]) never populates `fields` at all — the pass
/// would be a no-op there. It never synthesizes a `Field` from a method name
/// alone: linkage requires the field to already exist.
#[must_use]
pub fn extract_app_with_schema(source_tree: &Path, namespace: &str) -> (ModelGraph, SchemaReport) {
    let mut graph = crate::extract_app_with(source_tree, namespace);
    let mut report = SchemaReport {
        columns_from: "baseline-only",
        ..SchemaReport::default()
    };

    for table in parse_tables_dir(source_tree, &mut report) {
        report.tables_seen += 1;
        if let Some(model) = graph.models.iter_mut().find(|m| m.name == table.model_name) {
            report.tables_matched += 1;
            for field in table.fields {
                if !model.fields.iter().any(|f| f.name == field.name) {
                    model.fields.push(field);
                }
            }
        } else {
            report.unmatched_tables.push(table.table_name);
        }
    }
    report.unmatched_tables.sort();
    report.files_skipped.sort();

    for model in &mut graph.models {
        link_computed_fields(model);
    }

    (graph, report)
}

/// **D-AR-3.5 compute linkage.** For each `def compute_<x>` in `model.functions`,
/// if `model.fields` already has a field named `<x>` (schema-merged or
/// otherwise) with no `emitted_by` yet, set `field.emitted_by =
/// Some("compute_<x>")`.
///
/// This is grounded on BOTH sides — the column exists (schema stratum) AND
/// the def exists (method-name stratum, `class::extract_functions_from_body`)
/// — so it never synthesizes a [`Field`] from a method name alone: a
/// `compute_<x>` def with no matching `<x>` field links nothing and creates
/// nothing (the guardrail, pinned by
/// [`tests::link_computed_fields_does_not_synthesize_a_field_for_an_unmatched_compute_def`]).
/// An existing `emitted_by` (set by some future richer pass) is never
/// overwritten.
pub(crate) fn link_computed_fields(model: &mut Model) {
    let compute_targets: Vec<&str> = model
        .functions
        .iter()
        .filter_map(|f| f.name.strip_prefix("compute_"))
        .collect();
    for field in &mut model.fields {
        if field.emitted_by.is_none() && compute_targets.contains(&field.name.as_str()) {
            field.emitted_by = Some(format!("compute_{}", field.name));
        }
    }
}

/// Parse every baseline table file under `<root>/db/migrate/tables/`.
/// Deterministic: files are sorted before parsing (same discipline as
/// [`crate::parse`]'s walk). Unreadable / unrecognisable files land in the
/// report's `files_skipped`, not on the floor.
pub(crate) fn parse_tables_dir(source_tree: &Path, report: &mut SchemaReport) -> Vec<TableColumns> {
    let dir = source_tree.join("db/migrate/tables");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("rb"))
        .collect();
    files.sort();

    let mut tables = Vec::with_capacity(files.len());
    for path in files {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let Ok(src) = fs::read_to_string(&path) else {
            report.files_skipped.push(stem);
            continue;
        };
        match parse_table_source(&stem, &src) {
            Some(table) => tables.push(table),
            None => report.files_skipped.push(stem),
        }
    }
    tables
}

/// Parse one baseline table file's source. `None` when the file has no
/// `create_table` block (e.g. `base.rb`, the abstract helper class).
pub(crate) fn parse_table_source(table_name: &str, src: &str) -> Option<TableColumns> {
    let mut fields: Vec<Field> = Vec::new();
    let mut in_block = false;
    let mut saw_create_table = false;

    for raw in src.lines() {
        let line = raw.trim();
        if !in_block {
            if line.starts_with("create_table") || line.starts_with("create_unlogged_table") {
                saw_create_table = true;
                in_block = true;
                // Implicit primary key unless the create_table call opts out.
                if !line.contains("id: false") {
                    fields.push(column_field("id", "bigint", true));
                }
            }
            continue;
        }
        if line == "end" {
            // First `end` at block depth closes the `do |t|` block. The
            // baseline files nest nothing deeper inside it.
            break;
        }
        let Some(rest) = line.strip_prefix("t.") else {
            continue;
        };
        let (method, args) = match rest.split_once(char::is_whitespace) {
            Some((m, a)) => (m, a.trim()),
            None => (rest, ""),
        };
        match method {
            // Constraint / index facts, not columns.
            "index" | "foreign_key" | "check_constraint" | "exclusion_constraint" => {}
            // `t.timestamps precision: nil, null: true` → the pair.
            "timestamps" => {
                let not_null = parse_not_null(args);
                fields.push(column_field("created_at", "datetime", not_null));
                fields.push(column_field("updated_at", "datetime", not_null));
            }
            // `t.references :x, null: false, polymorphic: true` (alias
            // `belongs_to`) → `x_id` bigint, plus `x_type` string when
            // polymorphic.
            "references" | "belongs_to" => {
                if let Some(name) = first_symbol_arg(args) {
                    let not_null = parse_not_null(args);
                    fields.push(column_field(&format!("{name}_id"), "bigint", not_null));
                    if args.contains("polymorphic: true") {
                        fields.push(column_field(&format!("{name}_type"), "string", not_null));
                    }
                }
            }
            // `t.column :name, :type, opts` — the explicit form.
            "column" => {
                let mut symbols = args.split(',').map(str::trim);
                let name = symbols.next().and_then(symbol_token);
                let ty = symbols.next().and_then(symbol_token);
                if let (Some(name), Some(ty)) = (name, ty) {
                    fields.push(column_field(name, ty, parse_not_null(args)));
                }
            }
            // `t.<type> :name, opts` — the direct typed forms.
            m if COLUMN_TYPES.contains(&m) => {
                if let Some(name) = first_symbol_arg(args) {
                    fields.push(column_field(name, m, parse_not_null(args)));
                }
            }
            // Unknown t.* method: not a column declaration we recognise.
            // The closed COLUMN_TYPES list + this arm make additions an
            // explicit act (same discipline as the Predicate count-lock).
            _ => {}
        }
    }

    if !saw_create_table {
        return None;
    }
    Some(TableColumns {
        table_name: table_name.to_string(),
        model_name: model_name_for_table(table_name),
        fields,
    })
}

/// Build one column [`Field`]: `field_type` carries the DSL method name
/// verbatim; `not_null` only when the DSL says `null: false`.
fn column_field(name: &str, dsl_type: &str, not_null: bool) -> Field {
    Field {
        name: name.to_string(),
        field_type: Some(dsl_type.to_string()),
        not_null: if not_null { Some(true) } else { None },
        ..Field::default()
    }
}

/// `null: false` anywhere in the arg list → NOT NULL. Rails' default for
/// columns is nullable, so absence (or explicit `null: true`) is `false`.
fn parse_not_null(args: &str) -> bool {
    args.contains("null: false")
}

/// The first `:symbol` argument, e.g. `":subject, default: …"` → `subject`.
fn first_symbol_arg(args: &str) -> Option<&str> {
    args.split(',').next().and_then(symbol_token)
}

/// `":name"` → `name` (with surrounding whitespace tolerated).
fn symbol_token(part: &str) -> Option<&str> {
    part.trim().strip_prefix(':').map(str::trim_end)
}

/// Rails inflection, table → model: `snake_case` plural → `PascalCase`
/// singular (`work_packages` → `WorkPackage`). Only the last segment is
/// singularised. The rule chain covers the `OpenProject` baseline corpus;
/// genuinely irregular names belong in `IRREGULAR`, and a miss lands the
/// table in `unmatched_tables` (visible), never on a wrong model.
pub(crate) fn model_name_for_table(table: &str) -> String {
    /// Table names whose singular is not rule-derivable.
    const IRREGULAR: &[(&str, &str)] = &[
        ("news", "news"),
        ("meeting_agenda_item_series", "meeting_agenda_item_series"),
    ];

    let segments: Vec<&str> = table.split('_').collect();
    let mut out = String::new();
    let last = segments.len().saturating_sub(1);
    for (i, seg) in segments.iter().enumerate() {
        let word = if i == last {
            singularize(seg, table, IRREGULAR)
        } else {
            (*seg).to_string()
        };
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

/// Singularise one `snake_case` segment (the table's final word).
fn singularize(seg: &str, full_table: &str, irregular: &[(&str, &str)]) -> String {
    if let Some((_, singular)) = irregular.iter().find(|(t, _)| *t == full_table) {
        return (*singular).to_string();
    }
    if let Some(stem) = seg.strip_suffix("ies") {
        return format!("{stem}y");
    }
    for es_suffix in ["sses", "shes", "ches", "xes", "zes", "uses"] {
        if seg.ends_with(es_suffix) {
            return seg[..seg.len() - 2].to_string();
        }
    }
    seg.strip_suffix('s').unwrap_or(seg).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
class Tables::WorkPackages < Tables::Base
  def self.table(migration)
    create_table migration do |t|
      t.references :type, null: false, index: true, foreign_key: { on_delete: :cascade }
      t.string :subject, default: "", null: false
      t.text :description
      t.integer :done_ratio, default: nil, null: true
      t.timestamps precision: nil, null: true, index: true
      t.belongs_to :responsible
      t.boolean :schedule_manually, default: true, null: false
      t.references :reactable, polymorphic: true, null: false
      t.column :builtin, :boolean, default: false, null: false

      t.index %i[project_id updated_at]
      t.check_constraint "due_date >= start_date", name: "x"
    end
  end
end
"#;

    #[test]
    fn parses_the_dsl_forms() {
        let t = parse_table_source("work_packages", SAMPLE).expect("create_table block");
        assert_eq!(t.model_name, "WorkPackage");
        let names: Vec<&str> = t.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "id",
                "type_id",
                "subject",
                "description",
                "done_ratio",
                "created_at",
                "updated_at",
                "responsible_id",
                "schedule_manually",
                "reactable_id",
                "reactable_type",
                "builtin",
            ]
        );
        let by_name = |n: &str| t.fields.iter().find(|f| f.name == n).unwrap();
        // Implicit PK.
        assert_eq!(by_name("id").field_type.as_deref(), Some("bigint"));
        assert_eq!(by_name("id").not_null, Some(true));
        // references → _id bigint, null: false honoured.
        assert_eq!(by_name("type_id").field_type.as_deref(), Some("bigint"));
        assert_eq!(by_name("type_id").not_null, Some(true));
        // Plain nullable column: no positive fact.
        assert_eq!(by_name("description").field_type.as_deref(), Some("text"));
        assert_eq!(by_name("description").not_null, None);
        // Explicit null: true stays absent (nullable is the default).
        assert_eq!(by_name("done_ratio").not_null, None);
        // timestamps pair, honouring null: true.
        assert_eq!(
            by_name("created_at").field_type.as_deref(),
            Some("datetime")
        );
        assert_eq!(by_name("created_at").not_null, None);
        // belongs_to alias.
        assert_eq!(
            by_name("responsible_id").field_type.as_deref(),
            Some("bigint")
        );
        // Polymorphic pair — the PolyRef substrate declaring itself.
        assert_eq!(by_name("reactable_id").not_null, Some(true));
        assert_eq!(
            by_name("reactable_type").field_type.as_deref(),
            Some("string")
        );
        // t.column explicit form.
        assert_eq!(by_name("builtin").field_type.as_deref(), Some("boolean"));
        assert_eq!(by_name("builtin").not_null, Some(true));
    }

    #[test]
    fn id_false_suppresses_the_implicit_pk() {
        let src = "create_table migration, id: false do |t|\n  t.bigint :a_id\nend\n";
        let t = parse_table_source("a_b_joins", src).expect("block");
        assert_eq!(t.fields.len(), 1);
        assert_eq!(t.fields[0].name, "a_id");
    }

    #[test]
    fn base_helper_file_is_not_a_table() {
        assert!(parse_table_source("base", "class Tables::Base\nend\n").is_none());
    }

    #[test]
    fn inflection_covers_the_corpus_shapes() {
        for (table, model) in [
            ("work_packages", "WorkPackage"),
            ("statuses", "Status"),
            ("categories", "Category"),
            ("queries", "Query"),
            ("changes", "Change"),
            ("changesets", "Changeset"),
            ("news", "News"),
            ("attachments", "Attachment"),
            ("custom_fields", "CustomField"),
            ("issue_priorities", "IssuePriority"),
        ] {
            assert_eq!(model_name_for_table(table), model, "{table}");
        }
    }

    /// Corpus gate (same pattern as the crate's D-AR-4 gate): only runs
    /// with `OPENPROJECT_PATH` set. Pins the `WorkPackage` baseline column
    /// set — the oracle-diff ground truth.
    #[test]
    #[allow(clippy::print_stderr)] // diagnostic emission gated on env var (real-corpus gate)
    fn openproject_corpus_schema_gate() {
        let Ok(root) = std::env::var("OPENPROJECT_PATH") else {
            eprintln!("skipping: OPENPROJECT_PATH not set");
            return;
        };
        let (graph, report) = extract_app_with_schema(Path::new(&root), "openproject");
        assert_eq!(report.columns_from, "baseline-only");
        assert!(
            report.tables_seen >= 90,
            "expected ~99 baseline tables, saw {}",
            report.tables_seen
        );
        assert!(
            report.tables_matched >= 50,
            "matched only {}",
            report.tables_matched
        );
        eprintln!(
            "D-AR-3.5 schema gate: {} tables seen, {} matched, {} unmatched, {} files skipped",
            report.tables_seen,
            report.tables_matched,
            report.unmatched_tables.len(),
            report.files_skipped.len()
        );

        let wp = graph
            .models
            .iter()
            .find(|m| m.name == "WorkPackage")
            .expect("WorkPackage model");
        let cols: Vec<&str> = wp
            .fields
            .iter()
            .filter(|f| f.field_type.is_some())
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(cols.len(), 27, "baseline WorkPackage columns: {cols:?}");
        for expected in [
            "id",
            "type_id",
            "project_id",
            "subject",
            "description",
            "due_date",
            "category_id",
            "status_id",
            "assigned_to_id",
            "priority_id",
            "version_id",
            "author_id",
            "lock_version",
            "done_ratio",
            "estimated_hours",
            "created_at",
            "updated_at",
            "start_date",
            "responsible_id",
            "derived_estimated_hours",
            "schedule_manually",
            "parent_id",
            "duration",
            "ignore_non_working_days",
            "derived_remaining_hours",
            "derived_done_ratio",
            "project_phase_id",
        ] {
            assert!(cols.contains(&expected), "missing column {expected}");
        }
        let by_name = |n: &str| wp.fields.iter().find(|f| f.name == n).unwrap();
        assert_eq!(by_name("subject").field_type.as_deref(), Some("string"));
        assert_eq!(by_name("subject").not_null, Some(true));
        assert_eq!(by_name("done_ratio").field_type.as_deref(), Some("integer"));
        assert_eq!(
            by_name("done_ratio").not_null,
            None,
            "unset ≠ 0% — the oracle-diff bug"
        );
        assert_eq!(by_name("schedule_manually").not_null, Some(true));
    }

    // ────────────────── compute-linkage pass (D-AR-3.5) ──────────────────

    use ruff_spo_triplet::Function;

    /// A `compute_<x>` def whose class also has an `<x>` field (schema-merged
    /// or otherwise) gets linked: `field.emitted_by = Some("compute_<x>")`.
    #[test]
    fn link_computed_fields_links_matching_compute_def() {
        let mut model = Model {
            name: "WorkPackage".to_string(),
            fields: vec![Field {
                name: "total_hours".to_string(),
                ..Field::default()
            }],
            functions: vec![Function {
                name: "compute_total_hours".to_string(),
                ..Function::default()
            }],
            ..Model::default()
        };
        link_computed_fields(&mut model);
        assert_eq!(
            model.fields[0].emitted_by.as_deref(),
            Some("compute_total_hours")
        );
    }

    /// The guardrail: a `compute_<x>` def with NO matching `<x>` field must
    /// NOT synthesize a field — linkage requires the field to already exist
    /// on both sides (schema stratum + method-name stratum), never just the
    /// method name alone.
    #[test]
    fn link_computed_fields_does_not_synthesize_a_field_for_an_unmatched_compute_def() {
        let mut model = Model {
            name: "WorkPackage".to_string(),
            fields: Vec::new(),
            functions: vec![Function {
                name: "compute_total_hours".to_string(),
                ..Function::default()
            }],
            ..Model::default()
        };
        link_computed_fields(&mut model);
        assert!(
            model.fields.is_empty(),
            "a compute def with no matching field must not create one"
        );
    }

    /// A field with no matching `compute_<x>` def stays unlinked —
    /// `emitted_by` remains `None`.
    #[test]
    fn link_computed_fields_leaves_uncomputed_fields_alone() {
        let mut model = Model {
            name: "WorkPackage".to_string(),
            fields: vec![Field {
                name: "subject".to_string(),
                ..Field::default()
            }],
            functions: Vec::new(),
            ..Model::default()
        };
        link_computed_fields(&mut model);
        assert_eq!(model.fields[0].emitted_by, None);
    }
}
