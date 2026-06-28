//! `ruff_python_spo` — the Odoo/Python SPO frontend.
//!
//! This is the Python sibling of `ruff_ruby_spo`. Its ONLY job is to read an
//! Odoo model's Python AST (via `ruff_python_parser`) and fill a
//! [`ruff_spo_triplet::ModelGraph`]; [`ruff_spo_triplet::expand`] then yields
//! the same SPO triple shape every other frontend produces, and OGAR's
//! `ogar-from-ruff` lifts the same `ModelGraph` into `ogar_vocab::Class`.
//!
//! # What it extracts (the core-7 predicates)
//!
//! | Odoo construct                          | IR field             | predicate            |
//! | ---                                     | ---                  | ---                  |
//! | `class X(models.Model)` + `_name='a.b'` | [`Model::name`]      | `rdf:type ObjectType`|
//! | `x = fields.K(...)`                      | [`Field::name`]      | `rdf:type Property`  |
//! | `compute='_compute_x'`                  | [`Field::emitted_by`]| `emitted_by`         |
//! | `@api.depends('a','b')` on the computer | [`Field::depends_on`]| `depends_on` (fan-out)|
//! | `def m(self)`                           | [`Function::name`]   | `rdf:type Function` + `has_function` |
//! | `raise UserError(...)`                  | [`Function::raises`] | `raises` (`exc:`)    |
//! | `self.attr` / `record.attr`             | [`Function::reads`]  | `reads_field`        |
//! | `for r in self.line_ids:`               | [`Function::traverses`]| `traverses_relation`|
//!
//! Odoo relations (`target`/`inverse_name`), Selection enums
//! (`selection_value`), and `_inherit` edges (`inherits_from`) need predicate
//! variants the closed [`ruff_spo_triplet::Predicate`] enum does not yet carry;
//! they are a follow-up that extends the enum + IR carriers together.
//!
//! Model names are normalised dot→underscore (`account.move` → `account_move`)
//! so the IRI dot is unambiguously the model↔member separator.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use ruff_python_ast::Expr;
use ruff_spo_triplet::{Field, Function, Model, ModelGraph};

mod functions;
mod parse;
mod walk;

/// The IRI namespace prefix for every Odoo subject/object.
pub const NAMESPACE: &str = "odoo";

/// One class extracted from the AST, before the compute→depends join and the
/// model-name resolution that [`build_graph`] performs.
pub(crate) struct RawClass {
    /// The `_name = '...'` value, if declared (dotted, e.g. `account.move`).
    pub name: Option<String>,
    /// The `_inherit` value(s), normalised to a list (dotted).
    pub inherits: Vec<String>,
    /// Field declarations (`x = fields.K(...)`).
    pub fields: Vec<RawField>,
    /// Method declarations (`def ...`).
    pub methods: Vec<RawMethod>,
}

/// One `x = fields.K(...)` declaration. Only the bits the core-7 needs.
pub(crate) struct RawField {
    /// Field name.
    pub name: String,
    /// The `compute='_compute_x'` method name, if any.
    pub compute: Option<String>,
}

/// One `def ...` declaration with its extracted body facts.
pub(crate) struct RawMethod {
    /// Method name.
    pub name: String,
    /// `@api.depends('a','b')` field-path args (dotted, verbatim).
    pub depends: Vec<String>,
    /// `self.attr` / record-var attribute reads in the body.
    pub reads: Vec<String>,
    /// Exception type names raised in the body.
    pub raises: Vec<String>,
    /// Relation names traversed by `for r in self.rel:` loops.
    pub traverses: Vec<String>,
}

/// Extract a [`ModelGraph`] from a single Python source string.
///
/// A source that fails to parse contributes nothing (returns an empty graph),
/// mirroring the existing Odoo extractor's silent-skip invariant.
#[must_use]
pub fn extract_from_source(source: &str) -> ModelGraph {
    build_graph(&parse::parse_source(source), NAMESPACE)
}

/// Extract a [`ModelGraph`] from a source tree (recursively reads `*.py`),
/// using the given namespace.
#[must_use]
pub fn extract_with(root: &Path, namespace: &str) -> ModelGraph {
    let mut classes = Vec::new();
    collect_py(root, &mut classes);
    build_graph(&classes, namespace)
}

/// Extract a [`ModelGraph`] from a source tree under the default [`NAMESPACE`].
#[must_use]
pub fn extract(root: &Path) -> ModelGraph {
    extract_with(root, NAMESPACE)
}

/// Convenience: expand a graph and serialise it to ndjson ready for the SPO
/// store / od-ontology corpus.
#[must_use]
pub fn to_ndjson(graph: &ModelGraph) -> String {
    ruff_spo_triplet::to_ndjson(&ruff_spo_triplet::expand(graph))
}

/// Recursively collect every parseable class under `dir`.
fn collect_py(dir: &Path, out: &mut Vec<RawClass>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_py(&path, out);
        } else if path.extension().is_some_and(|e| e == "py")
            && let Ok(src) = fs::read_to_string(&path)
        {
            out.extend(parse::parse_source(&src));
        }
    }
}

/// Resolve a class's model identity: `_name` if present, else the first
/// `_inherit` (the extension idiom). Returns the underscore-normalised name.
fn resolve_name(class: &RawClass) -> Option<String> {
    class
        .name
        .clone()
        .or_else(|| class.inherits.first().cloned())
        .map(|dotted| dotted.replace('.', "_"))
}

/// Assemble raw classes into a [`ModelGraph`], performing the compute→depends
/// join (each computed field inherits its `_compute_` method's `@api.depends`
/// args — the per-(field × dep) `depends_on` fan-out) and merging classes that
/// resolve to the same model (Odoo `_inherit` reopens).
fn build_graph(classes: &[RawClass], namespace: &str) -> ModelGraph {
    let mut models: HashMap<String, Model> = HashMap::new();

    for class in classes {
        let Some(model_name) = resolve_name(class) else {
            continue;
        };

        let depends_by_method: HashMap<&str, &Vec<String>> = class
            .methods
            .iter()
            .map(|m| (m.name.as_str(), &m.depends))
            .collect();

        let fields = class.fields.iter().map(|f| Field {
            name: f.name.clone(),
            emitted_by: f.compute.clone(),
            depends_on: f
                .compute
                .as_deref()
                .and_then(|m| depends_by_method.get(m))
                .map(|deps| (*deps).clone())
                .unwrap_or_default(),
        });

        let methods = class.methods.iter().map(|m| Function {
            name: m.name.clone(),
            reads: m.reads.clone(),
            raises: m.raises.clone(),
            traverses: m.traverses.clone(),
        });

        let entry = models
            .entry(model_name.clone())
            .or_insert_with(|| Model::new(&model_name));
        entry.fields.extend(fields);
        entry.functions.extend(methods);
    }

    ModelGraph {
        namespace: namespace.to_string(),
        models: models.into_values().collect(),
    }
}

/// Pull the string value out of a string-literal expression.
pub(crate) fn expr_str(expr: &Expr) -> Option<String> {
    if let Expr::StringLiteral(s) = expr {
        Some(s.value.to_str().to_string())
    } else {
        None
    }
}

/// The bare identifier of a `Name` expression.
pub(crate) fn name_id(expr: &Expr) -> Option<&str> {
    if let Expr::Name(n) = expr {
        Some(n.id.as_str())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_spo_triplet::{Triple, expand};

    fn has(triples: &[Triple], s: &str, p: &str, o: &str) -> bool {
        triples.iter().any(|t| t.s == s && t.p == p && t.o == o)
    }

    // Real Odoo 19 fixture: addons/account/models/account_cash_rounding.py.
    const CASH_ROUNDING: &str = r#"
from odoo import models, fields, api, _
from odoo.exceptions import ValidationError


class AccountCashRounding(models.Model):
    _name = 'account.cash.rounding'
    _description = 'Account Cash Rounding'
    _check_company_auto = True

    name = fields.Char(string='Name', translate=True, required=True)
    rounding = fields.Float(string='Rounding Precision', required=True, default=0.01)
    strategy = fields.Selection([('biggest_tax', 'Modify tax amount'), ('add_invoice_line', 'Add a rounding line')],
        string='Rounding Strategy', default='add_invoice_line', required=True)
    profit_account_id = fields.Many2one('account.account', string='Profit Account', ondelete='restrict')
    loss_account_id = fields.Many2one('account.account', string='Loss Account', ondelete='restrict')
    rounding_method = fields.Selection(string='Rounding Method', required=True,
        selection=[('UP', 'Up'), ('DOWN', 'Down'), ('HALF-UP', 'Nearest')], default='HALF-UP')

    @api.constrains('rounding')
    def validate_rounding(self):
        for record in self:
            if record.rounding <= 0:
                raise ValidationError(_("Please set a strictly positive rounding value."))

    def round(self, amount):
        return float_round(amount, precision_rounding=self.rounding, rounding_method=self.rounding_method)

    def compute_difference(self, currency, amount):
        amount = currency.round(amount)
        difference = self.round(amount) - amount
        return currency.round(difference)
"#;

    #[test]
    fn cash_rounding_model_fields_functions_and_raise() {
        let graph = extract_from_source(CASH_ROUNDING);
        let t = expand(&graph);

        // Model classified.
        assert!(has(
            &t,
            "odoo:account_cash_rounding",
            "rdf:type",
            "ogit:ObjectType"
        ));

        // Every field is a Property.
        for f in [
            "name",
            "rounding",
            "strategy",
            "profit_account_id",
            "loss_account_id",
            "rounding_method",
        ] {
            assert!(
                has(
                    &t,
                    &format!("odoo:account_cash_rounding.{f}"),
                    "rdf:type",
                    "ogit:Property"
                ),
                "missing Property for field {f}"
            );
        }

        // Every method is a Function and belongs to the model.
        for m in ["validate_rounding", "round", "compute_difference"] {
            assert!(
                has(
                    &t,
                    &format!("odoo:account_cash_rounding.{m}"),
                    "rdf:type",
                    "ogit:Function"
                ),
                "missing Function for {m}"
            );
            assert!(
                has(
                    &t,
                    "odoo:account_cash_rounding",
                    "has_function",
                    &format!("odoo:account_cash_rounding.{m}")
                ),
                "missing has_function for {m}"
            );
        }

        // The guard raises ValidationError, namespaced exc:.
        assert!(has(
            &t,
            "odoo:account_cash_rounding.validate_rounding",
            "raises",
            "exc:ValidationError"
        ));

        // `for record in self: record.rounding` → a read on the loop-bound record.
        assert!(has(
            &t,
            "odoo:account_cash_rounding.validate_rounding",
            "reads_field",
            "odoo:account_cash_rounding.rounding"
        ));

        // No compute fields → no emitted_by edges.
        assert!(!t.iter().any(|tr| tr.p == "emitted_by"));

        // The ndjson round-trips through the closed-vocab parser.
        let nd = to_ndjson(&graph);
        let parsed = ruff_spo_triplet::from_ndjson(&nd).expect("ndjson round-trips");
        assert_eq!(parsed, t);
    }

    // Synthetic model exercising the compute→depends join + traverse.
    const COMPUTE: &str = r#"
from odoo import models, fields, api


class Foo(models.Model):
    _name = 'x.foo'
    tax = fields.Float()
    total = fields.Monetary(compute='_compute_total', store=True)

    @api.depends('line_ids.amount', 'tax')
    def _compute_total(self):
        for line in self.line_ids:
            self.total = line.amount
"#;

    #[test]
    fn compute_field_emits_emitted_by_depends_fanout_and_traverse() {
        let graph = extract_from_source(COMPUTE);
        let t = expand(&graph);

        // emitted_by: the field is written by its compute method.
        assert!(has(
            &t,
            "odoo:x_foo.total",
            "emitted_by",
            "odoo:x_foo._compute_total"
        ));

        // depends_on fan-out: one edge per @api.depends arg, subject = the field.
        assert!(has(
            &t,
            "odoo:x_foo.total",
            "depends_on",
            "odoo:x_foo.line_ids.amount"
        ));
        assert!(has(&t, "odoo:x_foo.total", "depends_on", "odoo:x_foo.tax"));

        // The compute body loops over self.line_ids.
        assert!(has(
            &t,
            "odoo:x_foo._compute_total",
            "traverses_relation",
            "odoo:x_foo.line_ids"
        ));
    }

    #[test]
    fn unparseable_source_yields_empty_graph() {
        let graph = extract_from_source("class Broken(:  # not valid python\n");
        assert!(graph.models.is_empty());
    }

    #[test]
    fn non_model_class_is_skipped() {
        let graph = extract_from_source("class Plain:\n    x = 1\n");
        assert!(graph.models.is_empty());
    }
}
