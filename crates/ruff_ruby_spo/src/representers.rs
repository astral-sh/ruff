//! `OpenProject` `APIv3` representer/schema field-declaration extractor.
//!
//! # What this is
//!
//! `app/**/*_representer.rb` files (Roar/Representable DSL) declare which
//! model attributes an `APIv3` endpoint exposes and, for `schema`-family
//! declarations, an explicit wire `type:` hint. This module is a
//! **std-only, presence-only, line-based scanner** over that closed
//! keyword vocabulary — no Ruby runtime, same discipline as
//! [`crate::schema`] and [`crate::views`]. Tier: Inferred, by
//! construction (a line scanner, not a parser).
//!
//! # The grammar (measured on the real corpus)
//!
//! ```text
//! property :id,
//! property :ignore_non_working_days
//! date_property :start_date,
//! date_time_property :created_at
//! formattable_property :description
//! associated_resource :category
//! associated_resource :author,
//! associated_project
//! resource :project_phase,
//! resources :customActions,
//! link :schema do
//! links :children,
//! schema :subject,
//!        type: "String",
//! schema_with_allowed_link :parent,
//!                          type: "WorkPackage",
//! schema_with_allowed_collection :priority,
//! ```
//!
//! A declaration line is: optional leading whitespace, then one of the
//! closed [`KEYWORDS`], then (for every keyword except
//! `associated_project`) whitespace and a Ruby symbol `:name`.
//! `associated_project` takes no symbol — its declared name is always
//! `"project"` (the RULES contract, not something derived from parsing).
//!
//! # Word-boundary correctness (`date_property` vs `property`)
//!
//! The keyword is matched by **maximal-munch identifier scan**, not
//! substring search: [`split_leading_ident`] reads the longest run of
//! identifier characters starting at the (whitespace-trimmed) start of
//! the line, THEN checks the whole token against [`KEYWORDS`]. This
//! means `date_property :start_date,` produces the token `"date_property"`
//! — which is checked as a whole against the keyword set and matches
//! `date_property`, never `property` — without any keyword-length
//! ordering trick. A line only ever yields at most one keyword match.
//!
//! # `type:` hint (Inferred, best-effort)
//!
//! After a matched declaration, [`find_type_hint_in_continuation`] scans
//! forward through the declaration's *continuation lines* — lines that
//! belong to the same call — for a `type: "T"` / `type: 'T'` pair. The
//! continuation window stops (exclusive) at the first line that is
//! blank, that itself matches another declaration keyword, or that
//! dedents to `end` / `def`. This is a heuristic, not a full expression
//! parse: it is intentionally simple, and a missed hint just means
//! `type_hint: None` — never a wrong value.
//!
//! # What is NOT captured (by design)
//!
//! - Anything about the representer's rendering logic (`exec_context:`,
//!   `getter`/`setter` blocks, `if:` guards, nested representer classes).
//!   Only the field-name + optional wire-type SET, per the presence-only
//!   doctrine shared with [`crate::views`].
//! - Declarations that span more than the keyword + one symbol on the
//!   same logical statement in unusual ways (e.g. a symbol computed by
//!   an expression instead of written literally) — the closed vocabulary
//!   here targets the measured corpus shape, not arbitrary Ruby.

use std::fs;
use std::path::{Path, PathBuf};

/// One field/schema declaration harvested from a `*_representer.rb` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepresenterDecl {
    /// The declaration keyword verbatim (`property`, `date_property`,
    /// `schema`, `link`, ...).
    pub keyword: String,
    /// The declared field name (the `:ident` after the keyword; for
    /// `link` / `links` the link rel name; always `"project"` for
    /// `associated_project`, which takes no symbol).
    pub name: String,
    /// The `type: "T"` hint when present within the declaration's
    /// continuation lines (schema-side mostly). `None` otherwise.
    pub type_hint: Option<String>,
}

/// One `*_representer.rb` file's declared fields, in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepresenterFieldSet {
    /// File path relative to the scan root, `/`-separated.
    pub file: String,
    /// Declarations in file order.
    pub decls: Vec<RepresenterDecl>,
}

/// The closed keyword vocabulary for representer field declarations.
/// Order is irrelevant — matching is by maximal-munch identifier scan +
/// exact-set membership (see the module doc), not substring / prefix
/// search, so `property` vs `date_property` never collide regardless of
/// list order.
const KEYWORDS: &[&str] = &[
    "property",
    "date_property",
    "date_time_property",
    "formattable_property",
    "associated_resource",
    "associated_project",
    "resource",
    "resources",
    "link",
    "links",
    "schema_with_allowed_link",
    "schema_with_allowed_collection",
    "schema",
];

/// Scan `root` recursively for `**/*_representer.rb` files and extract
/// their declared fields. Files that produce zero declarations are
/// omitted (presence-only, same discipline as [`crate::views`]'s
/// `views_with_hits` gate). Deterministic: files sorted by path, decls
/// in file (source) order.
#[must_use]
pub fn extract_representer_field_sets(root: &Path) -> Vec<RepresenterFieldSet> {
    let mut files = Vec::new();
    collect_representer_files(root, &mut files);

    let mut results = Vec::new();
    for path in &files {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let decls = extract_decls_from_source(&content);
        if decls.is_empty() {
            continue;
        }
        results.push(RepresenterFieldSet {
            file: relative_repr_path(root, path),
            decls,
        });
    }

    results.sort_by(|a, b| a.file.cmp(&b.file));
    results
}

/// Walk `dir` recursively, appending every file whose name ends in
/// `_representer.rb`. Entries are sorted before recursing so the result
/// is deterministic (same pattern as [`crate::views::extract_view_field_sets`]'s
/// `collect_erb_files`).
fn collect_representer_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_representer_files(&path, out);
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("_representer.rb"))
        {
            out.push(path);
        }
    }
}

/// `path` relative to `root`, rendered with `/` separators regardless of
/// platform — a stable identifier, not a filesystem path to reopen.
fn relative_repr_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Extract every [`RepresenterDecl`] from one file's source, in source
/// (line) order.
fn extract_decls_from_source(content: &str) -> Vec<RepresenterDecl> {
    let lines: Vec<&str> = content.lines().collect();
    let mut decls = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if let Some((keyword, name)) = match_declaration_line(line) {
            let type_hint = find_type_hint_in_continuation(&lines, idx);
            decls.push(RepresenterDecl {
                keyword,
                name,
                type_hint,
            });
        }
    }
    decls
}

/// If `line` (after trimming leading whitespace) opens with one of
/// [`KEYWORDS`] as a whole identifier token, return `(keyword, name)`.
/// `name` comes from the `:symbol` immediately following the keyword,
/// except for `associated_project`, whose name is always `"project"`
/// (the RULES contract — it takes no symbol).
fn match_declaration_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    let (token, rest) = split_leading_ident(trimmed)?;
    let keyword = *KEYWORDS.iter().find(|k| **k == token)?;
    let name = if keyword == "associated_project" {
        "project".to_string()
    } else {
        parse_symbol_name(rest)?
    };
    Some((keyword.to_string(), name))
}

/// Read the maximal leading run of Ruby-identifier characters
/// (`[A-Za-z_][A-Za-z0-9_]*`) from the start of `s`. Returns
/// `(token, rest)` where `rest` is everything after the token
/// (untouched — not trimmed). `None` if `s` doesn't start with an
/// identifier character.
fn split_leading_ident(s: &str) -> Option<(&str, &str)> {
    let mut iter = s.char_indices();
    let (_, first) = iter.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    let mut end = first.len_utf8();
    for (idx, c) in iter {
        if c.is_ascii_alphanumeric() || c == '_' {
            end = idx + c.len_utf8();
        } else {
            break;
        }
    }
    Some((&s[..end], &s[end..]))
}

/// Parse a leading Ruby symbol (`:name`) out of `rest` — the text
/// immediately following a matched keyword token. Skips leading
/// whitespace, requires a `:`, then reads the symbol's identifier via
/// [`split_leading_ident`].
fn parse_symbol_name(rest: &str) -> Option<String> {
    let trimmed = rest.trim_start();
    let after_colon = trimmed.strip_prefix(':')?;
    let (name, _) = split_leading_ident(after_colon)?;
    Some(name.to_string())
}

/// Ruby-identifier-forming characters for the `type:` word-boundary
/// check (no `@` — this scans plain hash-literal keys, never ivars).
fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Find a `type: "T"` / `type: 'T'` pair on `lines[decl_idx]` itself or
/// on its continuation lines. The continuation window stops (exclusive)
/// at the first line that is blank, that itself matches a declaration
/// keyword ([`match_declaration_line`]), or that dedents to `end` /
/// `def` — the same "belongs to this call" heuristic named in the
/// module doc.
fn find_type_hint_in_continuation(lines: &[&str], decl_idx: usize) -> Option<String> {
    if let Some(hint) = find_type_hint(lines[decl_idx]) {
        return Some(hint);
    }
    for line in lines.iter().skip(decl_idx + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "end" || trimmed.starts_with("def") {
            break;
        }
        if match_declaration_line(line).is_some() {
            break;
        }
        if let Some(hint) = find_type_hint(line) {
            return Some(hint);
        }
    }
    None
}

/// Scan `line` for a word-boundary `type:` key followed by a quoted
/// string value (`"..."` or `'...'`), returning the unquoted value.
fn find_type_hint(line: &str) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let needle: Vec<char> = "type".chars().collect();
    let n = needle.len();
    if chars.len() < n + 1 {
        return None;
    }
    for start in 0..=(chars.len() - n) {
        if chars[start..start + n] != needle[..] {
            continue;
        }
        if start > 0 && is_ident_char(chars[start - 1]) {
            continue;
        }
        let mut idx = start + n;
        if idx >= chars.len() || chars[idx] != ':' {
            continue;
        }
        idx += 1;
        while idx < chars.len() && chars[idx].is_whitespace() {
            idx += 1;
        }
        let quote = match chars.get(idx) {
            Some('"') => '"',
            Some('\'') => '\'',
            _ => continue,
        };
        idx += 1;
        let value_start = idx;
        while idx < chars.len() && chars[idx] != quote {
            idx += 1;
        }
        if idx >= chars.len() {
            continue;
        }
        return Some(chars[value_start..idx].iter().collect());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn scratch_dir(case: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ruff_ruby_spo_representers_{}_{case}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        root
    }

    /// Bare `property :name` — no options, no type hint.
    #[test]
    fn bare_property_is_captured() {
        let root = scratch_dir("bare_property");
        write_file(
            &root,
            "work_package_representer.rb",
            "class WorkPackageRepresenter\n  property :id,\nend\n",
        );

        let sets = extract_representer_field_sets(&root);
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].decls.len(), 1);
        assert_eq!(
            sets[0].decls[0],
            RepresenterDecl {
                keyword: "property".to_string(),
                name: "id".to_string(),
                type_hint: None,
            }
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// `date_property :x` must NOT also be counted (or misrouted) as a
    /// bare `property` — the maximal-munch keyword scan must land on the
    /// longer identifier `date_property`.
    #[test]
    fn date_property_is_not_double_matched_as_property() {
        let root = scratch_dir("date_property");
        write_file(&root, "baz_representer.rb", "date_property :start_date,\n");

        let sets = extract_representer_field_sets(&root);
        assert_eq!(sets.len(), 1);
        assert_eq!(
            sets[0].decls,
            vec![RepresenterDecl {
                keyword: "date_property".to_string(),
                name: "start_date".to_string(),
                type_hint: None,
            }]
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// `formattable_property` is its own keyword, distinct from
    /// `property` and `date_property`.
    #[test]
    fn formattable_property_is_captured() {
        let root = scratch_dir("formattable_property");
        write_file(
            &root,
            "issue_representer.rb",
            "formattable_property :description\n",
        );

        let sets = extract_representer_field_sets(&root);
        assert_eq!(sets.len(), 1);
        assert_eq!(
            sets[0].decls,
            vec![RepresenterDecl {
                keyword: "formattable_property".to_string(),
                name: "description".to_string(),
                type_hint: None,
            }]
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// `associated_resource` with and without a trailing comma (options
    /// continuation) both parse to the same shape here (options
    /// themselves are out of scope for this stratum).
    #[test]
    fn associated_resource_with_and_without_trailing_comma() {
        let root = scratch_dir("associated_resource");
        write_file(
            &root,
            "issue_representer.rb",
            "associated_resource :category\nassociated_resource :author,\n",
        );

        let sets = extract_representer_field_sets(&root);
        assert_eq!(sets.len(), 1);
        assert_eq!(
            sets[0].decls,
            vec![
                RepresenterDecl {
                    keyword: "associated_resource".to_string(),
                    name: "category".to_string(),
                    type_hint: None,
                },
                RepresenterDecl {
                    keyword: "associated_resource".to_string(),
                    name: "author".to_string(),
                    type_hint: None,
                },
            ]
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// `associated_project` takes no symbol — its name is always
    /// `"project"` by contract, not derived from parsing.
    #[test]
    fn associated_project_with_no_symbol_names_project() {
        let root = scratch_dir("associated_project");
        write_file(&root, "work_package_representer.rb", "associated_project\n");

        let sets = extract_representer_field_sets(&root);
        assert_eq!(sets.len(), 1);
        assert_eq!(
            sets[0].decls,
            vec![RepresenterDecl {
                keyword: "associated_project".to_string(),
                name: "project".to_string(),
                type_hint: None,
            }]
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// `schema :name,` followed by a continuation line carrying
    /// `type: "T"` captures the type hint.
    #[test]
    fn schema_with_type_hint_is_captured() {
        let root = scratch_dir("schema_type");
        write_file(
            &root,
            "work_package_representer.rb",
            "schema :subject,\n       type: \"String\",\n",
        );

        let sets = extract_representer_field_sets(&root);
        assert_eq!(sets.len(), 1);
        assert_eq!(
            sets[0].decls,
            vec![RepresenterDecl {
                keyword: "schema".to_string(),
                name: "subject".to_string(),
                type_hint: Some("String".to_string()),
            }]
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// `schema_with_allowed_collection` — no type hint present on this
    /// line, none found in continuation (end of file).
    #[test]
    fn schema_with_allowed_collection_is_captured() {
        let root = scratch_dir("schema_allowed_collection");
        write_file(
            &root,
            "work_package_representer.rb",
            "schema_with_allowed_collection :priority,\n",
        );

        let sets = extract_representer_field_sets(&root);
        assert_eq!(sets.len(), 1);
        assert_eq!(
            sets[0].decls,
            vec![RepresenterDecl {
                keyword: "schema_with_allowed_collection".to_string(),
                name: "priority".to_string(),
                type_hint: None,
            }]
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// `schema_with_allowed_link` picks up a `type:` hint from its
    /// continuation line, same as plain `schema`.
    #[test]
    fn schema_with_allowed_link_captures_type_hint() {
        let root = scratch_dir("schema_allowed_link");
        write_file(
            &root,
            "work_package_representer.rb",
            "schema_with_allowed_link :parent,\n                         type: \"WorkPackage\",\n",
        );

        let sets = extract_representer_field_sets(&root);
        assert_eq!(sets.len(), 1);
        assert_eq!(
            sets[0].decls,
            vec![RepresenterDecl {
                keyword: "schema_with_allowed_link".to_string(),
                name: "parent".to_string(),
                type_hint: Some("WorkPackage".to_string()),
            }]
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// `link` (block form) and `links` (plural, comma continuation) are
    /// both captured, each carrying its own keyword.
    #[test]
    fn link_and_links_are_captured_with_their_own_keyword() {
        let root = scratch_dir("link_links");
        write_file(
            &root,
            "work_package_representer.rb",
            "link :schema do\nend\nlinks :children,\n",
        );

        let sets = extract_representer_field_sets(&root);
        assert_eq!(sets.len(), 1);
        assert_eq!(
            sets[0].decls,
            vec![
                RepresenterDecl {
                    keyword: "link".to_string(),
                    name: "schema".to_string(),
                    type_hint: None,
                },
                RepresenterDecl {
                    keyword: "links".to_string(),
                    name: "children".to_string(),
                    type_hint: None,
                },
            ]
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// A `.rb` file whose name does NOT end in `_representer.rb` is
    /// ignored entirely, even though its content matches the grammar.
    #[test]
    fn non_representer_file_is_ignored() {
        let root = scratch_dir("non_representer");
        write_file(&root, "helper.rb", "property :sneaky\n");
        write_file(&root, "issue_representer.rb", "property :subject\n");

        let sets = extract_representer_field_sets(&root);
        assert_eq!(sets.len(), 1, "only the *_representer.rb file counts");
        assert_eq!(sets[0].file, "issue_representer.rb");
        assert_eq!(sets[0].decls[0].name, "subject");

        let _ = fs::remove_dir_all(&root);
    }

    /// A file that matches the `*_representer.rb` name but contains no
    /// recognised declaration lines produces no [`RepresenterFieldSet`]
    /// (presence-only — nothing to report is nothing reported).
    #[test]
    fn representer_file_with_no_declarations_is_omitted() {
        let root = scratch_dir("empty_representer");
        write_file(
            &root,
            "empty_representer.rb",
            "class EmptyRepresenter\nend\n",
        );

        let sets = extract_representer_field_sets(&root);
        assert!(
            sets.is_empty(),
            "no declarations means no field set: {sets:?}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// Full-grammar integration fixture: every keyword from the prompt's
    /// measured-corpus block, in one file, in source order — including
    /// both `type:`-bearing continuations.
    #[test]
    fn full_declaration_grammar_is_captured_in_source_order() {
        let root = scratch_dir("full_grammar");
        write_file(
            &root,
            "work_package_representer.rb",
            concat!(
                "property :id,\n",
                "property :ignore_non_working_days\n",
                "date_property :start_date,\n",
                "date_time_property :created_at\n",
                "formattable_property :description\n",
                "associated_resource :category\n",
                "associated_resource :author,\n",
                "associated_project\n",
                "resource :project_phase,\n",
                "resources :customActions,\n",
                "link :schema do\n",
                "links :children,\n",
                "schema :subject,\n",
                "       type: \"String\",\n",
                "schema_with_allowed_link :parent,\n",
                "                         type: \"WorkPackage\",\n",
                "schema_with_allowed_collection :priority,\n",
            ),
        );

        let sets = extract_representer_field_sets(&root);
        assert_eq!(sets.len(), 1);
        let decls = &sets[0].decls;
        assert_eq!(decls.len(), 15, "expected 15 declaration lines: {decls:?}");

        let expect = [
            ("property", "id", None),
            ("property", "ignore_non_working_days", None),
            ("date_property", "start_date", None),
            ("date_time_property", "created_at", None),
            ("formattable_property", "description", None),
            ("associated_resource", "category", None),
            ("associated_resource", "author", None),
            ("associated_project", "project", None),
            ("resource", "project_phase", None),
            ("resources", "customActions", None),
            ("link", "schema", None),
            ("links", "children", None),
            ("schema", "subject", Some("String")),
            ("schema_with_allowed_link", "parent", Some("WorkPackage")),
            ("schema_with_allowed_collection", "priority", None),
        ];
        for (decl, (keyword, name, type_hint)) in decls.iter().zip(expect.iter()) {
            assert_eq!(decl.keyword, *keyword);
            assert_eq!(decl.name, *name);
            assert_eq!(decl.type_hint.as_deref(), *type_hint);
        }

        let _ = fs::remove_dir_all(&root);
    }
}
