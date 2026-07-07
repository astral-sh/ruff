//! Codebook-DTO check — validate a harvested [`ModelGraph`] against an
//! **existing** concept codebook before any downstream lift.
//!
//! The DTO layering the transcode stack rests on is
//! `classid { ontology : codebook : label }`: the agnostic concept ids live
//! in the codebook (upstream, e.g. `ogar_vocab::class_ids::ALL`), while
//! app-specific labels stay in the consumer's private repo. A frontend
//! harvest arrives label-shaped (class names as the source spells them), so
//! the question this module answers is: **which harvested classes already
//! bind to an existing codebook concept, and which are unbound?**
//!
//! Running the check at the frontend seam matters for the `ActionDef` path:
//! `ogar-from-ruff::lift_actions` keys every lifted action on its subject
//! class, and `capability_registry::entries_from_actions` maps an unminted
//! concept to classid `0` so the hot-plug fuse (`UnknownClassid`) fires
//! downstream. This check surfaces the same gap *earlier and by name* — a
//! [`CodebookCheck::unbound`] entry at harvest time instead of a classid-0
//! bang at registration time. Minting is still the codebook owner's job;
//! this module only reports against what exists.
//!
//! Deliberately **data-driven and zero-dep**: the caller supplies the
//! codebook rows as `(&str, u16)` pairs (the exact shape upstream exports),
//! so this crate never grows a dependency on the codebook's home crate — the
//! dependency arrow stays codebook-owner → this crate, never the reverse.

use crate::ir::ModelGraph;

/// One harvested class successfully bound to an existing codebook concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodebookBinding {
    /// The class name exactly as the frontend harvested it (label-shaped).
    pub class_name: String,
    /// The `snake_case` concept key it resolved through (see [`concept_key`]).
    pub concept: String,
    /// The existing codebook concept id the key matched.
    pub class_id: u16,
}

/// Result of checking a harvested graph against an existing codebook.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CodebookCheck {
    /// Classes that resolved to an existing codebook concept.
    pub bound: Vec<CodebookBinding>,
    /// Class names with no codebook concept — each one is either a private
    /// label (bind it in the consumer's label layer) or a genuinely new
    /// concept (a deliberate mint in the codebook's home, never here).
    pub unbound: Vec<String>,
}

impl CodebookCheck {
    /// `true` when every harvested class bound to an existing concept — the
    /// green light that an `ActionDef` lift over this graph cannot produce a
    /// classid-0 registration.
    #[must_use]
    pub fn is_fully_bound(&self) -> bool {
        self.unbound.is_empty()
    }
}

/// The `snake_case` concept key for a harvested class name.
///
/// Strips a namespace prefix (anything up to the last `:`), then lowercases
/// PascalCase/camelCase with a `_` inserted at each case boundary —
/// `LabValue` → `lab_value`, `csharp:Invoice` → `invoice`. Names that are
/// already `snake_case` pass through unchanged (frontends normalise dots to
/// underscores before this point, per [`crate::ir::Model::name`]).
#[must_use]
pub fn concept_key(class_name: &str) -> String {
    let bare = class_name
        .rsplit_once(':')
        .map_or(class_name, |(_, tail)| tail);
    let mut key = String::with_capacity(bare.len() + 4);
    let mut prev_lower_or_digit = false;
    for ch in bare.chars() {
        if ch.is_uppercase() {
            if prev_lower_or_digit {
                key.push('_');
            }
            key.extend(ch.to_lowercase());
            prev_lower_or_digit = false;
        } else {
            key.push(ch);
            prev_lower_or_digit = ch.is_lowercase() || ch.is_ascii_digit();
        }
    }
    key
}

/// Check every model in a harvested graph against the existing codebook.
///
/// `codebook` rows are `(concept_name, concept_id)` pairs in the shape the
/// codebook's home crate exports; the caller passes them straight through.
/// Order of `bound`/`unbound` follows the graph's model order, so reports
/// are stable across runs.
#[must_use]
pub fn check_model_graph(graph: &ModelGraph, codebook: &[(&str, u16)]) -> CodebookCheck {
    let mut check = CodebookCheck::default();
    for model in &graph.models {
        let concept = concept_key(&model.name);
        match codebook.iter().find(|(name, _)| *name == concept) {
            Some((_, id)) => check.bound.push(CodebookBinding {
                class_name: model.name.clone(),
                concept,
                class_id: *id,
            }),
            None => check.unbound.push(model.name.clone()),
        }
    }
    check
}

#[cfg(test)]
mod tests {
    use super::{CodebookCheck, check_model_graph, concept_key};
    use crate::ir::{Model, ModelGraph};

    const CODEBOOK: &[(&str, u16)] = &[
        ("patient", 0x0901),
        ("diagnosis", 0x0902),
        ("lab_value", 0x0903),
    ];

    fn graph_of(names: &[&str]) -> ModelGraph {
        let mut graph = ModelGraph::new("csharp");
        for name in names {
            graph.models.push(Model {
                name: (*name).to_string(),
                ..Default::default()
            });
        }
        graph
    }

    #[test]
    fn concept_key_snake_cases_pascal_and_strips_namespace() {
        assert_eq!(concept_key("LabValue"), "lab_value");
        assert_eq!(concept_key("csharp:Invoice"), "invoice");
        assert_eq!(concept_key("already_snake"), "already_snake");
        assert_eq!(concept_key("Patient"), "patient");
    }

    #[test]
    fn bound_and_unbound_split_by_existing_codebook() {
        let graph = graph_of(&["Patient", "LabValue", "SomethingBespoke"]);
        let check = check_model_graph(&graph, CODEBOOK);
        assert_eq!(check.bound.len(), 2);
        assert_eq!(check.bound[0].concept, "patient");
        assert_eq!(check.bound[0].class_id, 0x0901);
        assert_eq!(check.bound[1].class_id, 0x0903);
        assert_eq!(check.unbound, vec!["SomethingBespoke".to_string()]);
        assert!(!check.is_fully_bound());
    }

    #[test]
    fn fully_bound_graph_is_the_actiondef_green_light() {
        let graph = graph_of(&["Patient", "Diagnosis"]);
        let check = check_model_graph(&graph, CODEBOOK);
        assert!(check.is_fully_bound());
        assert!(check.unbound.is_empty());
    }

    #[test]
    fn empty_graph_is_trivially_bound() {
        let check = check_model_graph(&ModelGraph::new("csharp"), CODEBOOK);
        assert_eq!(check, CodebookCheck::default());
        assert!(check.is_fully_bound());
    }
}
