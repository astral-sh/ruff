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
//! Not every Rails app squashes its history into a baseline, though —
//! Redmine (and most "classic" Rails apps) has no `db/migrate/tables/` at
//! all: the schema only ever exists as the *replay* of 300+ individual
//! `db/migrate/NNN_*.rb` / `db/migrate/<timestamp>_*.rb` files. §
//! "Two surfaces" below covers how [`extract_app_with_schema`] picks
//! between the two.
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
//! # Two surfaces (baseline squash vs classic replay)
//!
//! [`extract_app_with_schema`] sniffs the layout before parsing anything:
//!
//! - `<root>/db/migrate/tables/*.rb` exists → the **baseline** surface
//!   ([`parse_tables_dir`] / [`parse_table_source`]). Authoritative-tier:
//!   one file *is* one table's declarative final shape, no replay needed.
//! - otherwise, `<root>/db/migrate/*.rb` exists → the **classic** surface
//!   ([`parse_migrations_dir`] / [`apply_migration_source`]). **Inferred**
//!   tier, and approximate by construction: migrations are replayed in
//!   sorted-filename order (which, for both the legacy zero-padded
//!   sequence numbers and the modern 14-digit timestamp prefix, *is*
//!   migration application order) applying `create_table` + its column
//!   DSL, `change_table` (append-only — it can't be recreating a table
//!   that already exists), and `add_column`. `rename_column` /
//!   `remove_column` / `change_column` / `drop_table` are **counted, not
//!   replayed** ([`SchemaReport::unapplied_mutations`]) — correctly
//!   replaying them needs full evaluation-order tracking (a column
//!   renamed then a *new* column added under the old name; a table
//!   dropped and never recreated; …), which is out of scope for a line
//!   scanner. The count keeps the approximation honest instead of silent:
//!   a model's schema-merged fields via the classic surface are a
//!   superset of the true final schema (they can include columns that
//!   were later renamed away or removed), never a silent subset.
//!
//! # Scope (recorded honestly, conservation-ledger style)
//!
//! - **Baseline surface is baseline-only**: incremental migrations
//!   (`db/migrate/*.rb`, `modules/*/db/migrate/*.rb`) that
//!   `add_column`/`rename_column` after the squash are NOT replayed.
//!   [`SchemaReport::columns_from`] says so. (The classic surface, by
//!   contrast, exists *because* there is no squash to read instead.)
//! - Join tables and other tables with no matching AR class are counted in
//!   [`SchemaReport::unmatched_tables`], never silently dropped.
//! - `t.index` / `t.foreign_key` / `t.check_constraint` /
//!   `t.exclusion_constraint` lines are constraint/index facts, not
//!   columns — skipped here (a later slice can lift them).

use std::collections::BTreeMap;
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
    /// Provenance marker: which migration surface produced the columns —
    /// `"baseline-only"` (the `Tables::X` squash, no replay) or
    /// `"classic-migrations"` (replayed `db/migrate/*.rb`, approximate —
    /// see [`Self::unapplied_mutations`]).
    pub columns_from: &'static str,
    /// **Classic surface only.** Migration files under `db/migrate/`
    /// successfully read and replayed (`create_table` / `change_table` /
    /// `add_column` applied). Zero on the baseline surface.
    pub classic_migrations_scanned: usize,
    /// **Classic surface only.** Total count of `rename_column` /
    /// `remove_column` / `change_column` / `drop_table` statements seen
    /// across all scanned migrations — encountered, but deliberately NOT
    /// replayed (see the module doc's "Two surfaces" section). Zero on the
    /// baseline surface, where there is nothing to replay in the first
    /// place.
    pub unapplied_mutations: usize,
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
///
/// # Layout sniffing (baseline vs classic)
///
/// `<root>/db/migrate/tables/` is checked first: if it contains any `.rb`
/// file, this is the `OpenProject`-style squashed baseline and the
/// (unchanged) [`parse_tables_dir`] path runs. Otherwise, if
/// `<root>/db/migrate/` itself contains any `.rb` file, this is a classic
/// Rails app (Redmine and similar — no baseline squash, only the full
/// migration history) and [`parse_migrations_dir`] runs instead. Neither
/// directory existing leaves [`SchemaReport::tables_seen`] at zero, same
/// as before this fallback was added.
#[must_use]
pub fn extract_app_with_schema(source_tree: &Path, namespace: &str) -> (ModelGraph, SchemaReport) {
    let mut graph = crate::extract_app_with(source_tree, namespace);
    let use_classic_migrations = !dir_has_rb_files(&source_tree.join("db/migrate/tables"))
        && dir_has_rb_files(&source_tree.join("db/migrate"));
    let mut report = SchemaReport {
        columns_from: if use_classic_migrations {
            "classic-migrations"
        } else {
            "baseline-only"
        },
        ..SchemaReport::default()
    };

    let tables = if use_classic_migrations {
        parse_migrations_dir(source_tree, &mut report)
    } else {
        parse_tables_dir(source_tree, &mut report)
    };
    for table in tables {
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

/// `true` when `dir` exists and contains at least one `.rb` file
/// (non-recursive). The layout-sniffing probe [`extract_app_with_schema`]
/// uses to pick baseline-squash vs classic-replay.
fn dir_has_rb_files(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|e| e.path().extension().and_then(|e| e.to_str()) == Some("rb"))
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
        let (method, args) = split_method_args(rest);
        fields.extend(fields_from_column_dsl(method, args));
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

// ────────────────── classic `db/migrate/*.rb` replay ──────────────────

/// Parse every classic Rails migration file under `<root>/db/migrate/` (the
/// "no baseline squash" layout — Redmine and similar corpora). Files are
/// processed in **sorted filename order** — for classic Rails migrations,
/// filename order (the legacy zero-padded sequence number, e.g.
/// `001_setup.rb`, or the modern 14-digit timestamp prefix) *is* migration
/// application order, so replaying files in that order reproduces the
/// schema evolution `db/schema.rb` would otherwise have recorded, without
/// ever needing `schema.rb` to exist.
///
/// Returns one [`TableColumns`] per table name ever mentioned by a
/// `create_table` / `change_table` / `add_column`, sorted by table name for
/// deterministic output (migration-file order still governs each table's
/// *own* field order — see [`apply_migration_source`]). Approximate /
/// **Inferred**-tier by design: see the module doc's "Two surfaces"
/// section for what is and isn't replayed.
pub(crate) fn parse_migrations_dir(
    source_tree: &Path,
    report: &mut SchemaReport,
) -> Vec<TableColumns> {
    let dir = source_tree.join("db/migrate");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("rb"))
        .collect();
    files.sort();

    let mut tables: BTreeMap<String, Vec<Field>> = BTreeMap::new();
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
        report.classic_migrations_scanned += 1;
        apply_migration_source(&src, &mut tables, report);
    }

    tables
        .into_iter()
        .map(|(table_name, fields)| TableColumns {
            model_name: model_name_for_table(&table_name),
            table_name,
            fields,
        })
        .collect()
}

/// Replay one migration file's source against the running `tables` state.
///
/// Recognises, per top-level (non-block) line:
/// - `create_table "name"` / `create_table :name` (options ignored, except
///   `id: false` / `:id => false` suppressing the implicit PK) — opens a
///   column block AND **replaces** that table's column list (an idempotent
///   re-create further down the migration history), same as a fresh table.
/// - `change_table :name` — opens the SAME kind of column block, but never
///   clears: a `change_table` can only target a table that already exists,
///   so its `t.*` lines are batch `add_column`s (append-if-absent), not a
///   redefinition. (Redmine's `20180913072918_add_verify_peer_to_auth_sources.rb`
///   is exactly this shape.)
/// - `add_column :table, :col, :type` (or quoted-string table/col) —
///   appends the column if a same-named one isn't already present.
/// - `rename_column` / `remove_column` / `change_column` / `drop_table` —
///   counted in [`SchemaReport::unapplied_mutations`], never replayed (see
///   the module doc).
///
/// A column block closes at the first bare `end` line — the same
/// single-nesting-depth assumption [`parse_table_source`] documents holds
/// here too (verified against the full Redmine corpus: no `create_table` /
/// `change_table` body nests another `do |x| … end`).
fn apply_migration_source(
    src: &str,
    tables: &mut BTreeMap<String, Vec<Field>>,
    report: &mut SchemaReport,
) {
    let mut current_table: Option<String> = None;

    for raw in src.lines() {
        let line = raw.trim();

        if let Some(table_name) = &current_table {
            if line == "end" {
                current_table = None;
                continue;
            }
            let Some(rest) = line.strip_prefix("t.") else {
                continue;
            };
            let (method, args) = split_method_args(rest);
            let entry = tables.entry(table_name.clone()).or_default();
            for field in fields_from_column_dsl(method, args) {
                push_field_if_absent(entry, field);
            }
            continue;
        }

        if let Some((name, id_false)) = parse_create_table_opener(line) {
            tables.insert(
                name.clone(),
                if id_false {
                    Vec::new()
                } else {
                    vec![column_field("id", "bigint", true)]
                },
            );
            current_table = Some(name);
            continue;
        }
        if let Some(name) = parse_change_table_opener(line) {
            tables.entry(name.clone()).or_default();
            current_table = Some(name);
            continue;
        }

        if let Some((table, field)) = parse_add_column(line) {
            push_field_if_absent(tables.entry(table).or_default(), field);
            continue;
        }

        if [
            "rename_column",
            "remove_column",
            "change_column",
            "drop_table",
        ]
        .into_iter()
        .any(|keyword| is_mutation_call(line, keyword))
        {
            report.unapplied_mutations += 1;
        }
    }
}

/// Append `field` to `fields` unless a same-named column is already
/// present — the "appends if absent" discipline `add_column` /
/// `change_table` need (a migration re-adding a column it already added,
/// or a column the baseline-shape scan would otherwise duplicate).
fn push_field_if_absent(fields: &mut Vec<Field>, field: Field) {
    if !fields.iter().any(|f| f.name == field.name) {
        fields.push(field);
    }
}

/// Classic-migration `create_table` opener: `create_table "name", opts do
/// |t|` or `create_table :name, opts do |t|`. Returns the table name and
/// whether `id: false` / `:id => false` suppresses the implicit PK. `None`
/// for any other line — including the baseline DSL's `create_table
/// migration do |t|` form (`migration` is a bare local, not a table-name
/// literal), which never appears in classic migrations.
fn parse_create_table_opener(line: &str) -> Option<(String, bool)> {
    let rest = line.strip_prefix("create_table")?;
    // Guard against `create_tables`/`create_table_foo` identifiers: the
    // next byte must not continue an identifier.
    if rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let rest = rest.trim_start();
    let name = first_positional_name(rest)?;
    let id_false = rest.contains("id: false") || rest.contains(":id => false");
    Some((name.to_string(), id_false))
}

/// Classic-migration `change_table` opener: `change_table :name do |t|`.
/// Same shape as [`parse_create_table_opener`] minus the implicit-PK /
/// `id: false` bookkeeping (a `change_table` never (re)creates the table).
fn parse_change_table_opener(line: &str) -> Option<String> {
    let rest = line.strip_prefix("change_table")?;
    if rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
        return None;
    }
    first_positional_name(rest.trim_start()).map(str::to_string)
}

/// The first positional argument of a `create_table`/`change_table` call
/// tail (everything after the keyword), as a name token — stops at the
/// first `,` or whitespace, whichever comes first, so both
/// `"name", opts do |t|` and `:name do |t|` (no comma before `do`) extract
/// just the name.
fn first_positional_name(rest: &str) -> Option<&str> {
    let end = rest.find([',', ' ']).unwrap_or(rest.len());
    name_token(&rest[..end])
}

/// `add_column :table, :col, :type, opts` (or quoted-string table/col) → the
/// table name and the new [`Field`]. `None` for anything else (including a
/// malformed call missing the type, which the DSL requires).
fn parse_add_column(line: &str) -> Option<(String, Field)> {
    let rest = line.strip_prefix("add_column")?;
    if rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let mut parts = rest.trim_start().split(',').map(str::trim);
    let table = parts.next().and_then(name_token)?.to_string();
    let name = parts.next().and_then(name_token)?;
    let ty = parts.next().and_then(name_token)?;
    let not_null = parse_not_null(rest);
    Some((table, column_field(name, ty, not_null)))
}

/// `true` when `line` is a top-level classic-migration call to `keyword`
/// (`rename_column` / `remove_column` / `change_column` / `drop_table`) —
/// the keyword followed by whitespace or `(`, so e.g.
/// `change_column_default` / `change_column_null` (distinct DSL calls)
/// never false-match `change_column`.
fn is_mutation_call(line: &str, keyword: &str) -> bool {
    line.strip_prefix(keyword)
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_whitespace() || c == '('))
}

/// Split a `t.<method> <args>` line's remainder (post `t.` strip) into the
/// method name and its trimmed argument tail (`""` for a bare method like
/// `t.timestamps`).
fn split_method_args(rest: &str) -> (&str, &str) {
    match rest.split_once(char::is_whitespace) {
        Some((m, a)) => (m, a.trim()),
        None => (rest, ""),
    }
}

/// Given one `t.<method> <args>` line's parts, produce the column
/// [`Field`]s it declares (0, 1, or 2 — `references`/`belongs_to` can
/// yield an `_id` + `_type` pair, `timestamps` yields the
/// `created_at`/`updated_at` pair).
///
/// Shared by both migration surfaces: the `Tables::X` baseline DSL
/// ([`parse_table_source`]) and classic `db/migrate/*.rb` migrations
/// ([`apply_migration_source`]). Column/type tokens accept either a Ruby
/// symbol (`:name`) or a quoted string (`"name"` / `'name'`) via
/// [`name_token`] — classic migrations use both across Rails' history
/// (`t.column "container_id", :integer, …` is the dominant classic form);
/// the baseline DSL only ever used symbols, so this is a strict superset
/// and changes nothing for the baseline surface.
fn fields_from_column_dsl(method: &str, args: &str) -> Vec<Field> {
    match method {
        // Constraint / index facts, not columns.
        "index" | "foreign_key" | "check_constraint" | "exclusion_constraint" => Vec::new(),
        // `t.timestamps precision: nil, null: true` → the pair.
        "timestamps" => {
            let not_null = parse_not_null(args);
            vec![
                column_field("created_at", "datetime", not_null),
                column_field("updated_at", "datetime", not_null),
            ]
        }
        // `t.references :x, null: false, polymorphic: true` (alias
        // `belongs_to`) → `x_id` bigint, plus `x_type` string when
        // polymorphic.
        "references" | "belongs_to" => {
            let mut out = Vec::new();
            if let Some(name) = first_name_arg(args) {
                let not_null = parse_not_null(args);
                out.push(column_field(&format!("{name}_id"), "bigint", not_null));
                if args.contains("polymorphic: true") {
                    out.push(column_field(&format!("{name}_type"), "string", not_null));
                }
            }
            out
        }
        // `t.column :name, :type, opts` / `t.column "name", :type, opts` —
        // the explicit form.
        "column" => {
            let mut parts = args.split(',').map(str::trim);
            let name = parts.next().and_then(name_token);
            let ty = parts.next().and_then(name_token);
            match (name, ty) {
                (Some(name), Some(ty)) => vec![column_field(name, ty, parse_not_null(args))],
                _ => Vec::new(),
            }
        }
        // `t.<type> :name, opts` / `t.<type> "name", opts` — the direct
        // typed forms.
        m if COLUMN_TYPES.contains(&m) => first_name_arg(args)
            .map(|name| vec![column_field(name, m, parse_not_null(args))])
            .unwrap_or_default(),
        // Unknown t.* method: not a column declaration we recognise.
        // The closed COLUMN_TYPES list + this arm make additions an
        // explicit act (same discipline as the Predicate count-lock).
        _ => Vec::new(),
    }
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

/// `null: false` (the modern keyword-argument spelling) or `:null => false`
/// (the hash-rocket spelling classic migrations use — Redmine's
/// `db/migrate/001_setup.rb` predates Ruby 1.9 hash-literal shorthand)
/// anywhere in the arg list → NOT NULL. Rails' default for columns is
/// nullable, so absence (or explicit `null: true` / `:null => true`) is
/// `false`.
fn parse_not_null(args: &str) -> bool {
    args.contains("null: false") || args.contains(":null => false")
}

/// The first name argument — a `:symbol` or a quoted string — e.g.
/// `":subject, default: …"` → `subject`, or `"\"subject\", default: …"` →
/// `subject`.
fn first_name_arg(args: &str) -> Option<&str> {
    args.split(',').next().and_then(name_token)
}

/// One column/type token: a Ruby symbol (`:name`) or a quoted string
/// (`"name"` / `'name'`). `":name"` → `name`; `"\"name\""` → `name`
/// (surrounding whitespace tolerated on both forms).
fn name_token(part: &str) -> Option<&str> {
    let part = part.trim();
    if let Some(sym) = part.strip_prefix(':') {
        return Some(sym.trim_end());
    }
    for quote in ['"', '\''] {
        if let Some(rest) = part.strip_prefix(quote) {
            return rest.find(quote).map(|end| &rest[..end]);
        }
    }
    None
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

    // ────────────────── classic `db/migrate/*.rb` fallback (Redmine-style) ──────────────────

    use std::path::PathBuf;

    fn write_migration(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn scratch_dir(case: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ruff_ruby_spo_schema_classic_{}_{case}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        root
    }

    /// `create_table` with both `t.column "name", :type` (the dominant
    /// classic form) and the direct `t.<type> :name` / `t.<type> "name"`
    /// forms in the same block.
    #[test]
    fn classic_create_table_parses_column_and_typed_forms() {
        let root = scratch_dir("dsl-forms");
        write_migration(
            &root,
            "db/migrate/001_setup.rb",
            r#"
class Setup < ActiveRecord::Migration[4.2]
  def self.up
    create_table "widgets", :force => true do |t|
      t.column "name", :string, :default => "", :null => false
      t.integer "count"
      t.string :label
      t.text "notes"
    end
  end
end
"#,
        );

        let mut report = SchemaReport::default();
        let tables = parse_migrations_dir(&root, &mut report);
        assert_eq!(tables.len(), 1);
        let widgets = &tables[0];
        assert_eq!(widgets.table_name, "widgets");
        assert_eq!(widgets.model_name, "Widget");
        let names: Vec<&str> = widgets.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["id", "name", "count", "label", "notes"]);

        let by_name = |n: &str| widgets.fields.iter().find(|f| f.name == n).unwrap();
        assert_eq!(by_name("name").field_type.as_deref(), Some("string"));
        assert_eq!(by_name("name").not_null, Some(true));
        assert_eq!(by_name("count").field_type.as_deref(), Some("integer"));
        assert_eq!(by_name("label").field_type.as_deref(), Some("string"));
        assert_eq!(by_name("notes").field_type.as_deref(), Some("text"));
        assert_eq!(report.classic_migrations_scanned, 1);
        assert_eq!(report.unapplied_mutations, 0);

        let _ = fs::remove_dir_all(&root);
    }

    /// `t.timestamps` adds the `created_at`/`updated_at` pair, honouring
    /// `null: false`.
    #[test]
    fn classic_t_timestamps_adds_created_and_updated_at() {
        let root = scratch_dir("timestamps");
        write_migration(
            &root,
            "db/migrate/001_setup.rb",
            r#"
class Setup < ActiveRecord::Migration[4.2]
  def self.up
    create_table :widgets do |t|
      t.string :name
      t.timestamps null: false
    end
  end
end
"#,
        );

        let mut report = SchemaReport::default();
        let tables = parse_migrations_dir(&root, &mut report);
        let widgets = &tables[0];
        let names: Vec<&str> = widgets.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["id", "name", "created_at", "updated_at"]);
        let by_name = |n: &str| widgets.fields.iter().find(|f| f.name == n).unwrap();
        assert_eq!(by_name("created_at").not_null, Some(true));
        assert_eq!(by_name("updated_at").not_null, Some(true));

        let _ = fs::remove_dir_all(&root);
    }

    /// `t.references` adds the `_id` column (bigint), honouring `null: false`.
    #[test]
    fn classic_t_references_adds_id_column() {
        let root = scratch_dir("references");
        write_migration(
            &root,
            "db/migrate/001_setup.rb",
            r#"
class Setup < ActiveRecord::Migration[4.2]
  def self.up
    create_table :widgets do |t|
      t.references :project, null: false
    end
  end
end
"#,
        );

        let mut report = SchemaReport::default();
        let tables = parse_migrations_dir(&root, &mut report);
        let widgets = &tables[0];
        let names: Vec<&str> = widgets.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["id", "project_id"]);
        let by_name = |n: &str| widgets.fields.iter().find(|f| f.name == n).unwrap();
        assert_eq!(by_name("project_id").field_type.as_deref(), Some("bigint"));
        assert_eq!(by_name("project_id").not_null, Some(true));

        let _ = fs::remove_dir_all(&root);
    }

    /// A later `add_column` IS applied — appended to the table's running
    /// column list.
    #[test]
    fn classic_add_column_is_applied() {
        let root = scratch_dir("add-column");
        write_migration(
            &root,
            "db/migrate/001_setup.rb",
            r#"
class Setup < ActiveRecord::Migration[4.2]
  def self.up
    create_table "widgets", :force => true do |t|
      t.column "name", :string
    end
  end
end
"#,
        );
        write_migration(
            &root,
            "db/migrate/002_add_price.rb",
            r#"
class AddPrice < ActiveRecord::Migration[4.2]
  def self.up
    add_column :widgets, :price, :integer, :default => 0, :null => false
  end
end
"#,
        );

        let mut report = SchemaReport::default();
        let tables = parse_migrations_dir(&root, &mut report);
        let widgets = &tables[0];
        let names: Vec<&str> = widgets.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["id", "name", "price"]);
        let by_name = |n: &str| widgets.fields.iter().find(|f| f.name == n).unwrap();
        assert_eq!(by_name("price").field_type.as_deref(), Some("integer"));
        assert_eq!(by_name("price").not_null, Some(true));
        assert_eq!(report.classic_migrations_scanned, 2);
        assert_eq!(report.unapplied_mutations, 0);

        let _ = fs::remove_dir_all(&root);
    }

    /// `rename_column` is COUNTED, never applied — the old name survives
    /// untouched and the report's ledger increments.
    #[test]
    fn classic_rename_column_is_counted_not_applied() {
        let root = scratch_dir("rename-column");
        write_migration(
            &root,
            "db/migrate/001_setup.rb",
            r#"
class Setup < ActiveRecord::Migration[4.2]
  def self.up
    create_table "widgets", :force => true do |t|
      t.column "old_name", :string
    end
  end
end
"#,
        );
        write_migration(
            &root,
            "db/migrate/002_rename.rb",
            r#"
class Rename < ActiveRecord::Migration[4.2]
  def self.up
    rename_column :widgets, :old_name, :new_name
  end
end
"#,
        );

        let mut report = SchemaReport::default();
        let tables = parse_migrations_dir(&root, &mut report);
        let widgets = &tables[0];
        let names: Vec<&str> = widgets.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["id", "old_name"], "rename must NOT be applied");
        assert_eq!(report.unapplied_mutations, 1);

        let _ = fs::remove_dir_all(&root);
    }

    /// `remove_column` / `change_column` / `drop_table` are each counted,
    /// never applied — mirrors [`classic_rename_column_is_counted_not_applied`]
    /// for the other three mutation kinds in one file, including the
    /// `change_column_default`-must-not-false-match-`change_column` guard.
    #[test]
    fn classic_remove_change_drop_are_counted_not_applied() {
        let root = scratch_dir("mutations");
        write_migration(
            &root,
            "db/migrate/001_setup.rb",
            r#"
class Setup < ActiveRecord::Migration[4.2]
  def self.up
    create_table "widgets", :force => true do |t|
      t.column "name", :string
      t.column "legacy", :string
    end
    create_table "gadgets" do |t|
      t.string :name
    end
  end
end
"#,
        );
        write_migration(
            &root,
            "db/migrate/002_mutate.rb",
            r#"
class Mutate < ActiveRecord::Migration[4.2]
  def self.up
    remove_column :widgets, :legacy
    change_column :widgets, :name, :text
    change_column_default :widgets, :name, from: "", to: nil
    drop_table :gadgets
  end
end
"#,
        );

        let mut report = SchemaReport::default();
        let tables = parse_migrations_dir(&root, &mut report);
        let widgets = tables.iter().find(|t| t.table_name == "widgets").unwrap();
        let names: Vec<&str> = widgets.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            ["id", "name", "legacy"],
            "remove_column must NOT be applied"
        );
        assert_eq!(
            widgets
                .fields
                .iter()
                .find(|f| f.name == "name")
                .unwrap()
                .field_type
                .as_deref(),
            Some("string"),
            "change_column must NOT be applied"
        );
        assert!(
            tables.iter().any(|t| t.table_name == "gadgets"),
            "drop_table must NOT be applied"
        );
        assert_eq!(
            report.unapplied_mutations, 3,
            "remove_column + change_column + drop_table, NOT change_column_default"
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// A later `create_table` of the same table REPLACES its column list
    /// (idempotent re-create), rather than merging with the earlier one.
    #[test]
    fn classic_later_create_table_replaces_earlier_columns() {
        let root = scratch_dir("recreate");
        write_migration(
            &root,
            "db/migrate/001_setup.rb",
            r#"
class Setup < ActiveRecord::Migration[4.2]
  def self.up
    create_table "widgets" do |t|
      t.string :old_only
    end
  end
end
"#,
        );
        write_migration(
            &root,
            "db/migrate/002_recreate.rb",
            r#"
class Recreate < ActiveRecord::Migration[4.2]
  def self.up
    create_table "widgets", :force => true do |t|
      t.string :new_only
    end
  end
end
"#,
        );

        let mut report = SchemaReport::default();
        let tables = parse_migrations_dir(&root, &mut report);
        let widgets = &tables[0];
        let names: Vec<&str> = widgets.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            ["id", "new_only"],
            "later create_table must replace, not merge"
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// `change_table` never clears — it's an `add_column` batch against an
    /// existing table, not a redefinition.
    #[test]
    fn classic_change_table_appends_without_clearing() {
        let root = scratch_dir("change-table");
        write_migration(
            &root,
            "db/migrate/001_setup.rb",
            r#"
class Setup < ActiveRecord::Migration[4.2]
  def self.up
    create_table "widgets" do |t|
      t.string :name
    end
  end
end
"#,
        );
        write_migration(
            &root,
            "db/migrate/002_change_table.rb",
            r#"
class AddVerifyPeer < ActiveRecord::Migration[5.2]
  def change
    change_table :widgets do |t|
      t.boolean :verify_peer, default: true, null: false
    end
  end
end
"#,
        );

        let mut report = SchemaReport::default();
        let tables = parse_migrations_dir(&root, &mut report);
        let widgets = &tables[0];
        let names: Vec<&str> = widgets.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["id", "name", "verify_peer"]);
        let by_name = |n: &str| widgets.fields.iter().find(|f| f.name == n).unwrap();
        assert_eq!(by_name("verify_peer").not_null, Some(true));

        let _ = fs::remove_dir_all(&root);
    }

    /// Files are replayed in SORTED filename order, never directory/creation
    /// order — write `002` to disk before `001` and confirm the schema still
    /// reflects `001` having run first.
    #[test]
    fn classic_sorted_file_order_respected() {
        let root = scratch_dir("order");
        write_migration(
            &root,
            "db/migrate/002_recreate.rb",
            r#"
class Recreate < ActiveRecord::Migration[4.2]
  def self.up
    create_table "widgets", :force => true do |t|
      t.string :new_only
    end
  end
end
"#,
        );
        write_migration(
            &root,
            "db/migrate/001_setup.rb",
            r#"
class Setup < ActiveRecord::Migration[4.2]
  def self.up
    create_table "widgets" do |t|
      t.string :old_only
    end
  end
end
"#,
        );

        let mut report = SchemaReport::default();
        let tables = parse_migrations_dir(&root, &mut report);
        let widgets = &tables[0];
        let names: Vec<&str> = widgets.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            ["id", "new_only"],
            "001 then 002 must replay in filename order regardless of write order"
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// Regression: an `OpenProject`-style baseline squash (`db/migrate/tables/`)
    /// alongside a classic `db/migrate/*.rb` file still routes to the
    /// (unchanged) baseline path — the classic file is completely ignored,
    /// not merely deprioritised.
    #[test]
    fn op_layout_takes_priority_over_classic_migrate_dir() {
        let root = scratch_dir("op-priority");
        write_migration(
            &root,
            "db/migrate/tables/widgets.rb",
            r#"
class Tables::Widgets < Tables::Base
  def self.table(migration)
    create_table migration do |t|
      t.string :name, null: false
    end
  end
end
"#,
        );
        write_migration(
            &root,
            "db/migrate/001_setup.rb",
            r#"
class Setup < ActiveRecord::Migration[4.2]
  def self.up
    create_table "gadgets" do |t|
      t.string :name
    end
  end
end
"#,
        );

        let (_, report) = extract_app_with_schema(&root, "redmine");
        assert_eq!(report.columns_from, "baseline-only");
        assert_eq!(
            report.tables_seen, 1,
            "only the baseline table; gadgets must be ignored entirely"
        );
        assert_eq!(report.classic_migrations_scanned, 0);
        assert_eq!(report.unmatched_tables, vec!["widgets".to_string()]);

        let _ = fs::remove_dir_all(&root);
    }

    /// End-to-end: no baseline squash present, only classic migrations —
    /// [`extract_app_with_schema`] routes to the classic surface and merges
    /// the replayed columns into the matching model.
    #[test]
    fn classic_layout_merges_columns_into_matching_model() {
        let root = scratch_dir("classic-merge");
        write_migration(
            &root,
            "app/models/widget.rb",
            "class Widget < ActiveRecord::Base\nend\n",
        );
        write_migration(
            &root,
            "db/migrate/001_setup.rb",
            r#"
class Setup < ActiveRecord::Migration[4.2]
  def self.up
    create_table "widgets", :force => true do |t|
      t.column "name", :string, :null => false
    end
  end
end
"#,
        );

        let (graph, report) = extract_app_with_schema(&root, "redmine");
        assert_eq!(report.columns_from, "classic-migrations");
        assert_eq!(report.classic_migrations_scanned, 1);
        assert_eq!(report.tables_matched, 1);
        let widget = graph
            .models
            .iter()
            .find(|m| m.name == "Widget")
            .expect("Widget model");
        let names: Vec<&str> = widget.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["id", "name"]);
        assert_eq!(
            widget.fields[1].not_null,
            Some(true),
            "old hash-rocket `:null => false` must be honoured"
        );

        let _ = fs::remove_dir_all(&root);
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
