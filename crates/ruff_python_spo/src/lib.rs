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
//! Odoo relations carry their cardinality too: `target` (comodel),
//! `inverse_name` (One2many inverse), and `relation_kind`
//! (`many2one`/`one2many`/`many2many`) — the last separates a Many2one
//! (scalar FK) from a Many2many (join table), which `target`/`inverse_name`
//! alone cannot. Selection enums (`selection_value`) and `_inherit` edges
//! (`inherits_from`) still need predicate variants the closed
//! [`ruff_spo_triplet::Predicate`] enum does not yet carry; they are a
//! follow-up that extends the enum + IR carriers together.
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
    /// Relational comodel (raw dotted, e.g. `res.partner`), if relational.
    pub target: Option<String>,
    /// One2many inverse field name, if applicable.
    pub inverse_name: Option<String>,
    /// Relational cardinality, lowercased (`many2one` / `one2many` /
    /// `many2many`), if relational.
    pub relation_kind: Option<String>,
    /// Scalar field constructor, lowercased (`char` / `integer` / `float`
    /// / `monetary` / `boolean` / `date` / `datetime` / `selection` / …).
    /// `None` for relational fields (their kind rides `relation_kind`).
    pub field_type: Option<String>,
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

/// Assemble raw classes into a [`ModelGraph`].
///
/// Classes that resolve to the same model (Odoo `_inherit` reopens across
/// files) are accumulated first, then each model's `compute`→`@api.depends`
/// join runs at the **model** level — so a computed field declared in one
/// reopen still gets the `depends_on` fan-out from its `_compute_*` method
/// even when the method lives in a sibling reopen (the per-(field × dep)
/// fan-out). Building the join per-class would drop those cross-reopen deps.
fn build_graph(classes: &[RawClass], namespace: &str) -> ModelGraph {
    // Phase 1: accumulate raw fields + methods per resolved model name.
    let mut by_model: HashMap<String, (Vec<&RawField>, Vec<&RawMethod>)> = HashMap::new();
    for class in classes {
        let Some(model_name) = resolve_name(class) else {
            continue;
        };
        let entry = by_model.entry(model_name).or_default();
        entry.0.extend(&class.fields);
        entry.1.extend(&class.methods);
    }

    // Phase 2: build each model with a model-level compute→depends join.
    let models = by_model
        .into_iter()
        .map(|(name, (fields, methods))| {
            let depends_by_method: HashMap<&str, &Vec<String>> = methods
                .iter()
                .map(|m| (m.name.as_str(), &m.depends))
                .collect();
            Model {
                fields: fields
                    .iter()
                    .map(|f| Field {
                        name: f.name.clone(),
                        emitted_by: f.compute.clone(),
                        depends_on: f
                            .compute
                            .as_deref()
                            .and_then(|m| depends_by_method.get(m))
                            .map(|deps| (*deps).clone())
                            .unwrap_or_default(),
                        target: f.target.clone(),
                        inverse_name: f.inverse_name.clone(),
                        relation_kind: f.relation_kind.clone(),
                        field_type: f.field_type.clone(),
                    })
                    .collect(),
                functions: methods
                    .iter()
                    .map(|m| Function {
                        name: m.name.clone(),
                        reads: m.reads.clone(),
                        raises: m.raises.clone(),
                        traverses: m.traverses.clone(),
                    })
                    .collect(),
                name,
                ..Default::default()
            }
        })
        .collect();

    ModelGraph {
        namespace: namespace.to_string(),
        models,
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

        // Scalar fields carry their Odoo constructor as `field_type`, so a
        // downstream lift can upgrade an untyped scalar into a concrete
        // typed wrapper (Char→str, Float→float, Selection→selection).
        assert!(has(
            &t,
            "odoo:account_cash_rounding.name",
            "field_type",
            "char"
        ));
        assert!(has(
            &t,
            "odoo:account_cash_rounding.rounding",
            "field_type",
            "float"
        ));
        assert!(has(
            &t,
            "odoo:account_cash_rounding.strategy",
            "field_type",
            "selection"
        ));
        // …but relational fields do NOT — their kind rides `relation_kind`,
        // so the two predicates never double-emit for one field.
        assert!(!t.iter().any(|tr| tr.s
            == "odoo:account_cash_rounding.profit_account_id"
            && tr.p == "field_type"));

        // Many2one comodels surface as `target` (raw dotted, not an IRI).
        assert!(has(
            &t,
            "odoo:account_cash_rounding.profit_account_id",
            "target",
            "account.account"
        ));
        assert!(has(
            &t,
            "odoo:account_cash_rounding.loss_account_id",
            "target",
            "account.account"
        ));
        // …and carry their cardinality so a Many2one can't be mistaken for a
        // Many2many (both are target-only, no inverse).
        assert!(has(
            &t,
            "odoo:account_cash_rounding.profit_account_id",
            "relation_kind",
            "many2one"
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
    line_ids = fields.One2many('x.line', 'foo_id')

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

        // One2many relation: target (raw dotted comodel) + inverse_name +
        // cardinality.
        assert!(has(&t, "odoo:x_foo.line_ids", "target", "x.line"));
        assert!(has(&t, "odoo:x_foo.line_ids", "inverse_name", "foo_id"));
        assert!(has(&t, "odoo:x_foo.line_ids", "relation_kind", "one2many"));

        // Codex P2: `line.amount` keeps its relation hop (line ← self.line_ids).
        assert!(has(
            &t,
            "odoo:x_foo._compute_total",
            "reads_field",
            "odoo:x_foo.line_ids.amount"
        ));
        // Codex P2: the store target `self.total = ...` is NOT a read.
        assert!(!has(
            &t,
            "odoo:x_foo._compute_total",
            "reads_field",
            "odoo:x_foo.total"
        ));
    }

    // Reopen scenario: a model split across classes via `_inherit`.
    const REOPEN: &str = r#"
from odoo import models, fields, api


class FooBase(models.Model):
    _name = 'x.foo'
    total = fields.Monetary(compute='_compute_total', store=True)


class FooExt(models.Model):
    _inherit = 'x.foo'

    @api.depends('amount', 'tax')
    def _compute_total(self):
        self.total = self.amount + self.tax
"#;

    #[test]
    fn reopened_model_joins_depends_across_classes() {
        // The computed field is declared in FooBase; its @api.depends method
        // lives in the FooExt reopen. The compute→depends join must run at the
        // model level, after the two classes merge (Codex P2 #3).
        let graph = extract_from_source(REOPEN);
        let t = expand(&graph);
        assert!(has(
            &t,
            "odoo:x_foo.total",
            "emitted_by",
            "odoo:x_foo._compute_total"
        ));
        assert!(has(
            &t,
            "odoo:x_foo.total",
            "depends_on",
            "odoo:x_foo.amount"
        ));
        assert!(has(&t, "odoo:x_foo.total", "depends_on", "odoo:x_foo.tax"));
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

    // Many2one vs Many2many are byte-identical on (target, inverse_name) —
    // both carry a comodel and no inverse. `relation_kind` is the only
    // signal that separates them, so the OGAR lift can map M2O → BelongsTo
    // and M2M → HasAndBelongsToMany rather than guessing from the `_id`/
    // `_ids` name convention.
    const RELATION_KINDS: &str = r#"
from odoo import models, fields


class ResUsers(models.Model):
    _name = 'res.users'
    partner_id = fields.Many2one('res.partner')
    group_ids = fields.Many2many('res.groups')
    log_ids = fields.One2many('res.users.log', 'user_id')
"#;

    #[test]
    fn relation_kind_separates_many2one_from_many2many() {
        let graph = extract_from_source(RELATION_KINDS);
        let t = expand(&graph);

        // All three carry their comodel as `target`.
        assert!(has(&t, "odoo:res_users.partner_id", "target", "res.partner"));
        assert!(has(&t, "odoo:res_users.group_ids", "target", "res.groups"));
        assert!(has(
            &t,
            "odoo:res_users.log_ids",
            "target",
            "res.users.log"
        ));

        // …but only `relation_kind` tells the cardinality apart.
        assert!(has(
            &t,
            "odoo:res_users.partner_id",
            "relation_kind",
            "many2one"
        ));
        assert!(has(
            &t,
            "odoo:res_users.group_ids",
            "relation_kind",
            "many2many"
        ));
        assert!(has(
            &t,
            "odoo:res_users.log_ids",
            "relation_kind",
            "one2many"
        ));

        // Many2many has no inverse field (its inverse is a join table);
        // only the One2many carries `inverse_name`.
        assert!(!has(
            &t,
            "odoo:res_users.group_ids",
            "inverse_name",
            "user_id"
        ));
        assert!(has(&t, "odoo:res_users.log_ids", "inverse_name", "user_id"));

        // The ndjson round-trips through the closed-vocab parser (relation_kind
        // is now a recognised predicate).
        let nd = to_ndjson(&graph);
        let parsed = ruff_spo_triplet::from_ndjson(&nd).expect("ndjson round-trips");
        assert_eq!(parsed, t);
    }
}
