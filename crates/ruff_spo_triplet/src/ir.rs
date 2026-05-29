//! The language-agnostic intermediate representation.
//!
//! A frontend's ONLY job is to fill a [`ModelGraph`] from its own AST. The
//! Python/Odoo frontend reads `@api.depends`, compute-method bodies, and
//! `raise` statements; a Ruby/Rails frontend reads ActiveRecord
//! associations, `validate`/`validates` callbacks, and memoized methods.
//! Both produce the SAME `ModelGraph`, so [`crate::expand`] yields the same
//! triple shape.
//!
//! This IR is intentionally dumb: plain owned data, no behaviour, no
//! parsing. It is the contract seam between "how language X exposes its
//! model graph" and "what the SPO store consumes".
//!
//! # Mapping cheat-sheet
//!
//! | IR field                | Odoo (Python)                       | Rails (Ruby)                                  |
//! | ---                     | ---                                 | ---                                           |
//! | [`Model::name`]         | `_name` / class (`account.move`)    | ActiveRecord class (`WorkPackage`)            |
//! | [`Field::name`]         | `fields.X = fields.Type(...)`       | DB column / `attribute` / `attr_accessor`     |
//! | [`Field::depends_on`]   | `@api.depends("a.b.c")` args        | `belongs_to`/`has_many` chains a method reads |
//! | [`Field::emitted_by`]   | `compute="_compute_x"`              | memoized/derived method assigning the attr    |
//! | [`Function::name`]      | `def _compute_x(self)`              | `def compute_x` / instance method             |
//! | [`Function::reads`]     | attribute reads in body             | `self.x` / association reads in body          |
//! | [`Function::raises`]    | `raise UserError(...)`              | `raise`, `errors.add`, `validates`            |
//! | [`Function::traverses`] | `for r in self.line_ids:`           | `work_package.children.each`                  |

use serde::{Deserialize, Serialize};

/// The whole extracted model graph for one source tree.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ModelGraph {
    /// The IRI namespace prefix for subjects/objects (e.g. `"odoo"`,
    /// `"openproject"`). Subjects become `"<namespace>:<model>.<member>"`.
    pub namespace: String,
    /// Every model/entity in the tree.
    pub models: Vec<Model>,
}

impl ModelGraph {
    /// Create an empty graph for the given namespace.
    #[must_use]
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            models: Vec::new(),
        }
    }
}

/// One model / entity (Odoo model, Rails ActiveRecord class).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Model {
    /// The model identity — kept exactly as the source names it, except
    /// that dots in Odoo model names (`account.move`) are normalised to
    /// underscores by convention so the IRI dot is unambiguously the
    /// model↔member separator. The frontend owns this normalisation.
    pub name: String,
    /// Fields / attributes / columns.
    pub fields: Vec<Field>,
    /// Methods / functions.
    pub functions: Vec<Function>,
}

/// One field / attribute / column.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Field {
    /// Field name (e.g. `amount_total`).
    pub name: String,
    /// Declared compute dependencies — dotted relation paths
    /// (`line_ids.balance`). Emitted as `depends_on` (Authoritative).
    pub depends_on: Vec<String>,
    /// The function that computes/writes this field, if any. Emitted as
    /// `(field, emitted_by, fn)` (Authoritative).
    pub emitted_by: Option<String>,
}

/// One method / function.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Function {
    /// Function name (e.g. `_compute_amount`).
    pub name: String,
    /// Field names this function reads in its body. Emitted as
    /// `reads_field` (Inferred).
    pub reads: Vec<String>,
    /// Exception/error type names this function raises. Emitted as
    /// `raises` against the `exc:` namespace (Authoritative).
    pub raises: Vec<String>,
    /// Relation names this function traverses (loop targets). Emitted as
    /// `traverses_relation` (Inferred).
    pub traverses: Vec<String>,
}

impl Model {
    /// Convenience constructor for a bare model with no members yet.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fields: Vec::new(),
            functions: Vec::new(),
        }
    }
}
