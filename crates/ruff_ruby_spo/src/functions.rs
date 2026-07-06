//! Class-body method-def walker — extracts [`ruff_spo_triplet::Function`]
//! records from `def name … end` blocks (D-AR-3.5).
//!
//! # What it captures
//!
//! - **`Function::name`** — the method name from `Node::Def` (instance
//!   methods only; `Node::Defs` for `def self.foo` is class-method
//!   territory, treated separately).
//! - **`Function::raises`** — every `raise X[.new(…)]` statement
//!   reachable from the body. The exception type name is the constant
//!   passed to `raise`; falls back to a marker for `raise <expr>` forms
//!   that aren't a static constant.
//! - **`Function::traverses`** — every `<relation>.each` / `for r in
//!   <relation>` / association walk on `self.<rel>` whose name matches
//!   one of the class's declared associations. The walker takes the
//!   `known_relations` slice so it can filter (no relation declared →
//!   no traversal recorded; conservative for D-AR-3.5).
//! - **`Function::reads`** — `self.<field>` reads and bare attribute
//!   reads (no scope analysis — every `Send { recv: self, method: foo }`
//!   that is not a write or a mutator counts as a read of `foo`).
//! - **`Function::writes`** — `self.<field> = …` setter calls. The `=`
//!   suffix on the method name marks the assignment; the field is the name
//!   without the `=`. Plain instance-var assignment (`@x = …`) is NOT a
//!   write — it is local memoization, not an AR attribute.
//! - **`Function::calls`** — `ActiveRecord` lifecycle-mutator dispatches
//!   (`save` / `update` / `destroy` / …) on any receiver, recorded as
//!   `"<receiver>.<method>"`. Only the closed mutator set is captured — the
//!   body-pass triage (E-ACCIDENTAL-IMPERATIVE / OGAR F17) needs "does this
//!   method call a writer", not every call.
//!
//! Together, `writes` + `calls` are the **command-shape** facts that let the
//! triage split a method into query (read-only) vs command (mutates state).
//!
//! # What it doesn't capture (deferred)
//!
//! - Class methods (`def self.foo`, `class << self` blocks).
//! - `errors.add(...)` → `raises ActiveRecord::RecordInvalid` mapping.
//! - Block-form callbacks (`before_save do |r| … end`) — the block
//!   body's def-less statements aren't reachable here.
//! - Receiver-walks that span multiple hops (`self.project.members`).
//! - Op-assign writes on non-self-attribute targets (`x ||= y`, `@x += 1`) —
//!   only a `self.<field>` receiver is recognised. `self.x ||= y` is captured
//!   as `writes` + `guarded_writes` (the J1 default idiom); `self.x += y` is
//!   captured as `writes` + `reads` (a read-modify-write, not a guard);
//!   `self.x &&= y` is captured as a plain `writes` (present-guarded — the
//!   mirror of J1, deliberately not `guarded_writes`).
//!
//! These all land in follow-up D-AR-3.6 (method bodies are deep; the
//! 80/20 here is method NAMES + leaf `raise` + association walks).

use lib_ruby_parser::Node;
use ruff_spo_triplet::Function;

use crate::Declaration;

/// Walk a class body and produce one [`Function`] per `def`. The
/// `declarations` slice lets the body walker filter `traverses_relation`
/// candidates to known association names (per the Inferred-tier
/// I-RAILS-RELATION-WALK convention).
#[must_use]
pub(crate) fn extract_functions_from_body(
    body: Option<&Node>,
    declarations: &[Declaration],
) -> (Vec<Function>, Vec<Function>) {
    let Some(body) = body else {
        return (Vec::new(), Vec::new());
    };
    let known_relations = collect_known_relations(declarations);
    let mut out = Vec::new();
    // Visibility-at-def, parallel to `out` (true = public where the def appeared).
    let mut vis: Vec<bool> = Vec::new();
    // Symbol-form statements in source order: `private :a` / `public :a`. The
    // LAST statement about a name wins (`private :x` then `public :x` ⇒ public).
    let mut sym_overrides: Vec<(String, bool)> = Vec::new();
    // Default visibility, threaded so it persists across transparent `begin/end`
    // wrappers (a bare `private` before a nested begin still governs later defs).
    let mut default_public = true;
    walk_class_body_for_defs(
        body,
        &known_relations,
        &mut out,
        &mut vis,
        &mut sym_overrides,
        &mut default_public,
    );
    // Ruby visibility split: `private`/`protected` helpers (`set_project`,
    // `authorize`, …) are NOT routable actions and stay out of the first
    // (public/action) vec — but their body facts matter (Rails callbacks
    // conventionally target private methods; OGAR F17 body triage), so they
    // are returned as the second (helpers) vec instead of dropped. Final
    // visibility = visibility-at-def, then each symbol-form override applied in
    // order (so `public :x` re-publishes a method an earlier `private :x` hid).
    for (name, is_public) in &sym_overrides {
        for (i, f) in out.iter().enumerate() {
            if &f.name == name {
                vis[i] = *is_public;
            }
        }
    }
    let mut public_fns = Vec::new();
    let mut helper_fns = Vec::new();
    for (f, public) in out.into_iter().zip(vis) {
        if public {
            public_fns.push(f);
        } else {
            helper_fns.push(f);
        }
    }
    (public_fns, helper_fns)
}

/// Pre-compute the set of relation names declared on the class so
/// `traverses_relation` extraction can filter body-walked sends.
fn collect_known_relations(decls: &[Declaration]) -> Vec<String> {
    let mut names = Vec::new();
    for d in decls {
        if let Declaration::Association(a) = d {
            names.push(a.name.clone());
        }
    }
    names
}

/// Recurse into Begin / Module / Class wrappers AND `class_methods do`
/// blocks; for each **public** `Node::Def` encountered, extract one Function.
///
/// Ruby method visibility is tracked so non-public helpers are not harvested as
/// actions: a bare `private` / `protected` statement flips subsequent defs to
/// non-public (a bare `public` flips back); `private def foo …` marks just that
/// inline def; `private :foo` records an ordered `sym_overrides` entry the caller
/// applies by name (last wins, so a later `public :foo` re-publishes it).
fn walk_class_body_for_defs(
    node: &Node,
    known_relations: &[String],
    out: &mut Vec<Function>,
    vis: &mut Vec<bool>,
    sym_overrides: &mut Vec<(String, bool)>,
    default_public: &mut bool,
) {
    match node {
        // Both an implicit statement sequence (`Node::Begin`) and an explicit
        // `begin … end` (`Node::KwBegin`) are transparent to visibility:
        // `default_public` flows in and out unchanged, so a bare `private` inside
        // a nested begin still governs later class-body siblings.
        Node::Begin(b) => walk_body_stmts(
            &b.statements,
            known_relations,
            out,
            vis,
            sym_overrides,
            default_public,
        ),
        Node::KwBegin(b) => walk_body_stmts(
            &b.statements,
            known_relations,
            out,
            vis,
            sym_overrides,
            default_public,
        ),
        Node::Def(d) => {
            let mut func = Function {
                name: d.name.clone(),
                reads: Vec::new(),
                raises: Vec::new(),
                traverses: Vec::new(),
                writes: Vec::new(),
                calls: Vec::new(),
                guarded_writes: Vec::new(),
            };
            if let Some(fn_body) = d.body.as_deref() {
                walk_method_body(fn_body, known_relations, &mut func);
            }
            dedup_in_place(&mut func.reads);
            dedup_in_place(&mut func.raises);
            dedup_in_place(&mut func.traverses);
            dedup_in_place(&mut func.writes);
            dedup_in_place(&mut func.calls);
            dedup_in_place(&mut func.guarded_writes);
            out.push(func);
            // Visibility-at-def: the default in force where this `def` appeared.
            vis.push(*default_public);
        }
        Node::Block(blk) => {
            // `class_methods do … end` / `included do … end` blocks wrap `def`
            // nodes. The block INHERITS the enclosing default visibility, but its
            // own bare markers are scoped to the block — save/restore so a
            // `private` inside the block does not leak out to later class-body
            // siblings (D-AR-3.6 will split class-method discovery off).
            if let Some(body) = blk.body.as_deref() {
                let saved = *default_public;
                walk_class_body_for_defs(
                    body,
                    known_relations,
                    out,
                    vis,
                    sym_overrides,
                    default_public,
                );
                *default_public = saved;
            }
        }
        _ => {}
    }
}

/// Walk a linear statement sequence (a class body or a transparent `begin/end`),
/// tracking `default_public` visibility across siblings and emitting one
/// [`Function`] per public `def`.
fn walk_body_stmts(
    stmts: &[Node],
    known_relations: &[String],
    out: &mut Vec<Function>,
    vis: &mut Vec<bool>,
    sym_overrides: &mut Vec<(String, bool)>,
    default_public: &mut bool,
) {
    for stmt in stmts {
        if let Node::Send(s) = stmt {
            if s.recv.is_none()
                && matches!(s.method_name.as_str(), "private" | "protected" | "public")
            {
                if s.args.is_empty() {
                    // Bare marker: flips the default for subsequent defs.
                    *default_public = s.method_name == "public";
                    continue;
                }
                // Argument forms.
                let is_public = s.method_name == "public";
                for arg in &s.args {
                    match arg {
                        // `private :a` / `public :a` — record an ordered
                        // override applied by name (last wins).
                        Node::Sym(sym) => {
                            sym_overrides.push((sym.name.to_string_lossy(), is_public));
                        }
                        // `private def foo …` / `public def foo …` — emit the
                        // inline def with THIS explicit visibility.
                        Node::Def(_) => {
                            let before = out.len();
                            walk_class_body_for_defs(
                                arg,
                                known_relations,
                                out,
                                vis,
                                sym_overrides,
                                default_public,
                            );
                            for v in &mut vis[before..] {
                                *v = is_public;
                            }
                        }
                        _ => {}
                    }
                }
                continue;
            }
        }
        walk_class_body_for_defs(
            stmt,
            known_relations,
            out,
            vis,
            sym_overrides,
            default_public,
        );
    }
}

/// Walk one method body, populating `func.reads` / `raises` / `traverses`.
/// J1 (`writes_if_blank`) — detect the write-if-blank / write-if-nil idiom that
/// distinguishes a **schema default** from a **`normalizes` transform** inside
/// the otherwise-degenerate `SelfMap` recipe (`W ⊆ R`). See
/// `.claude/knowledge/fuzzy-recipe-codebook.md` §5 (J1).
///
/// Two shapes are recognised, both LOCAL to one `If`/`IfMod` node (no
/// dominator analysis — a write buried under a nested unrelated conditional is
/// deliberately NOT claimed, keeping the fact Authoritative):
/// - `self.X = default if  self.X.blank? | .nil? | .empty?` — guarded branch is
///   `if_true`, cond is a blank-test on `X`.
/// - `self.X = default unless self.X.present?` — guarded branch is `if_false`
///   (`unless present` ≡ `if blank`), cond is a present-test on `X`.
///
/// When the guarded branch writes the same field the cond tests, that field is
/// pushed to `func.guarded_writes` (it is ALSO recorded in `writes` by the
/// normal walk; `dedup` handles the overlap).
fn detect_guarded_default(
    cond: &Node,
    if_true: Option<&Node>,
    if_false: Option<&Node>,
    func: &mut Function,
) {
    // `if X.blank?` guards the true-branch on X.
    if let (Some(field), Some(branch)) = (blank_guarded_field(cond), if_true) {
        claim_if_writes(branch, field, func);
    }
    // `unless X.present?` (≡ `if X.blank?`) guards the false-branch on X.
    if let (Some(field), Some(branch)) = (present_guarded_field(cond), if_false) {
        claim_if_writes(branch, field, func);
    }
}

/// `self.X` or bare `X` (implicit-self attribute) → `Some("X")`; else `None`.
fn attr_of_self(node: &Node) -> Option<&str> {
    if let Node::Send(s) = node
        && s.args.is_empty()
        && is_attr_ident(&s.method_name)
        && matches!(s.recv.as_deref(), Some(Node::Self_(_)) | None)
    {
        return Some(&s.method_name);
    }
    None
}

/// `self.X.blank?` / `X.nil?` / `self.X.empty?` → the guarded field `X`.
fn blank_guarded_field(cond: &Node) -> Option<&str> {
    if let Node::Send(s) = cond
        && matches!(s.method_name.as_str(), "blank?" | "nil?" | "empty?")
    {
        return s.recv.as_deref().and_then(attr_of_self);
    }
    None
}

/// `self.X.present?` → `X` (the false-branch, i.e. `… unless X.present?`).
fn present_guarded_field(cond: &Node) -> Option<&str> {
    if let Node::Send(s) = cond
        && s.method_name == "present?"
    {
        return s.recv.as_deref().and_then(attr_of_self);
    }
    None
}

/// If `branch` contains a direct `self.<field> = …` write (through transparent
/// `begin`/statement wrappers only — NOT into nested conditionals), record
/// `field` as a guarded (default) write.
fn claim_if_writes(branch: &Node, field: &str, func: &mut Function) {
    match branch {
        Node::Send(s)
            if matches!(s.recv.as_deref(), Some(Node::Self_(_)))
                && s.method_name.strip_suffix('=').is_some_and(is_attr_ident)
                && s.method_name.strip_suffix('=') == Some(field) =>
        {
            func.guarded_writes.push(field.to_string());
        }
        Node::Begin(b) => {
            for st in &b.statements {
                claim_if_writes(st, field, func);
            }
        }
        Node::KwBegin(b) => {
            for st in &b.statements {
                claim_if_writes(st, field, func);
            }
        }
        _ => {}
    }
}

fn walk_method_body(node: &Node, known_relations: &[String], func: &mut Function) {
    match node {
        Node::Begin(b) => {
            for stmt in &b.statements {
                walk_method_body(stmt, known_relations, func);
            }
        }
        // `raise X` / `raise X.new(...)` / `raise X, ...`.
        Node::Send(s) if s.method_name == "raise" && s.recv.is_none() => {
            if let Some(arg) = s.args.first() {
                if let Some(exc_name) = exception_type_name(arg) {
                    func.raises.push(exc_name);
                }
            }
        }
        // `self.<x>` — write (`self.x = …`), mutator call (`self.save`),
        // or plain attribute read (`self.x`), in that priority order.
        Node::Send(s) if matches!(s.recv.as_deref(), Some(Node::Self_(_))) => {
            let method = s.method_name.as_str();
            if let Some(field) = method.strip_suffix('=')
                && is_attr_ident(field)
            {
                // `self.<field> = …` — the setter call: a write of `<field>`.
                // The `is_attr_ident` guard excludes comparison operators
                // (`==`, `<=`, `>=`, `===`) and `[]=`, which also end in `=`
                // but are not setters — without it, `self == other` would
                // record a bogus write of a field named `=`.
                func.writes.push(field.to_string());
            } else if is_ar_mutator(method) {
                // `self.save` / `self.update(...)` — lifecycle mutator on self.
                func.calls.push(format!("self.{method}"));
            } else if is_attr_ident(method) {
                // `self.<field>` — a plain attribute read (operator self-sends
                // such as `self == other` are neither a read nor a write).
                func.reads.push(s.method_name.clone());
            }
            // Recurse into args (the RHS of a write, or call/read args, may
            // themselves contain a raise/read/write/call).
            for arg in &s.args {
                walk_method_body(arg, known_relations, func);
            }
        }
        // `self.<field> ||= v` — the nil/false-guarded default write. Same J1
        // family as `self.x = v if self.x.blank?` (a falsy test is the same
        // "absent" guard for this purpose): recorded as BOTH `writes` and
        // `guarded_writes` (the latter always a subset of the former, same
        // invariant `claim_if_writes` upholds for the `If`/`IfMod` shape).
        // Non-self-attribute targets (`x ||= v`, `@x ||= v`) are left alone,
        // consistent with `is_attr_ident`/`attr_of_self` elsewhere.
        Node::OrAsgn(o) => {
            if let Some(field) = attr_of_self(&o.recv) {
                let field = field.to_string();
                func.writes.push(field.clone());
                func.guarded_writes.push(field);
            }
            walk_method_body(&o.value, known_relations, func);
        }
        // `self.<field> += v` (and the other op-assign operators) — a
        // read-modify-write, NOT a guarded default: the field's current value
        // is read to compute the new one, so both `reads` and `writes` are
        // recorded (no `guarded_writes` — there is no blank/nil test here).
        Node::OpAsgn(o) => {
            if let Some(field) = attr_of_self(&o.recv) {
                func.reads.push(field.to_string());
                func.writes.push(field.to_string());
            }
            walk_method_body(&o.value, known_relations, func);
        }
        // `self.<field> &&= v` — the PRESENT-guarded write (assigns only when
        // the field is already truthy). A write, but the mirror image of the
        // J1 "absent" guard — deliberately NOT `guarded_writes` (that predicate
        // means default-when-blank). RHS is still walked for facts.
        Node::AndAsgn(a) => {
            if let Some(field) = attr_of_self(&a.recv) {
                func.writes.push(field.to_string());
            }
            walk_method_body(&a.value, known_relations, func);
        }
        // `<relation>.each` / `<relation>.<m>` — association walks, plus any
        // `ActiveRecord` lifecycle mutator dispatched on a non-self receiver
        // (`order.update`, `User.create`, bare `save`).
        Node::Send(s) => {
            if is_ar_mutator(&s.method_name) {
                func.calls.push(format!(
                    "{}.{}",
                    receiver_label(s.recv.as_deref()),
                    s.method_name
                ));
            }
            if let Some(rel) = traversed_relation(s, known_relations) {
                func.traverses.push(rel);
            }
            if let Some(recv) = s.recv.as_deref() {
                walk_method_body(recv, known_relations, func);
            }
            for arg in &s.args {
                walk_method_body(arg, known_relations, func);
            }
        }
        // `for r in <rel>` — classic for-loop traversal.
        Node::For(f) => {
            if let Some(rel) = node_relation_name(&f.iteratee, known_relations) {
                func.traverses.push(rel);
            }
            if let Some(body) = f.body.as_deref() {
                walk_method_body(body, known_relations, func);
            }
        }
        // Walk into structural wrappers without changing func state.
        Node::Block(blk) => {
            if let Some(body) = blk.body.as_deref() {
                walk_method_body(body, known_relations, func);
            }
            walk_method_body(&blk.call, known_relations, func);
        }
        Node::If(i) => {
            // J1 guard detection (blank/nil-guarded default write) runs on the
            // block-form `if` too — same (cond, if_true, if_false) shape.
            detect_guarded_default(&i.cond, i.if_true.as_deref(), i.if_false.as_deref(), func);
            if let Some(b) = i.if_true.as_deref() {
                walk_method_body(b, known_relations, func);
            }
            if let Some(b) = i.if_false.as_deref() {
                walk_method_body(b, known_relations, func);
            }
            walk_method_body(&i.cond, known_relations, func);
        }
        // `stmt if cond` / `stmt unless cond` — the postfix modifier form.
        // `if_true` is set for `if`, `if_false` for `unless`; never both.
        // Same shape as `Node::If`, just without a block body.
        Node::IfMod(i) => {
            // J1: `self.x = default if self.x.blank?` is the canonical
            // write-if-blank (schema-default) idiom — detect it here.
            detect_guarded_default(&i.cond, i.if_true.as_deref(), i.if_false.as_deref(), func);
            if let Some(b) = i.if_true.as_deref() {
                walk_method_body(b, known_relations, func);
            }
            if let Some(b) = i.if_false.as_deref() {
                walk_method_body(b, known_relations, func);
            }
            walk_method_body(&i.cond, known_relations, func);
        }
        Node::Case(c) => {
            for arm in &c.when_bodies {
                walk_method_body(arm, known_relations, func);
            }
            if let Some(b) = c.else_body.as_deref() {
                walk_method_body(b, known_relations, func);
            }
        }
        Node::Ensure(e) => {
            if let Some(b) = e.body.as_deref() {
                walk_method_body(b, known_relations, func);
            }
            if let Some(b) = e.ensure.as_deref() {
                walk_method_body(b, known_relations, func);
            }
        }
        Node::Rescue(r) => {
            if let Some(b) = r.body.as_deref() {
                walk_method_body(b, known_relations, func);
            }
            for arm in &r.rescue_bodies {
                walk_method_body(arm, known_relations, func);
            }
        }
        Node::RescueBody(r) => {
            if let Some(b) = r.body.as_deref() {
                walk_method_body(b, known_relations, func);
            }
            if let Some(e) = r.exc_list.as_deref() {
                walk_method_body(e, known_relations, func);
            }
        }
        Node::Return(r) => {
            for arg in &r.args {
                walk_method_body(arg, known_relations, func);
            }
        }
        Node::Lvasgn(a) => {
            if let Some(v) = a.value.as_deref() {
                walk_method_body(v, known_relations, func);
            }
        }
        Node::Ivasgn(a) => {
            if let Some(v) = a.value.as_deref() {
                walk_method_body(v, known_relations, func);
            }
        }
        _ => {}
    }
}

/// The closed set of `ActiveRecord` lifecycle mutators. A call to one of
/// these marks a method as a *command* (it writes persistent state) rather
/// than a *query*. The body-pass triage (E-ACCIDENTAL-IMPERATIVE / OGAR F17)
/// groups methods by "calls a writer" — this IS that set. Not every call is
/// captured into `Function::calls`; only a dispatch of one of these verbs.
const AR_MUTATORS: &[&str] = &[
    "create",
    "create!",
    "update",
    "update!",
    "update_all",
    "update_attribute",
    "update_column",
    "update_columns",
    "destroy",
    "destroy!",
    "destroy_all",
    "delete",
    "delete_all",
    "save",
    "save!",
    "insert",
    "insert_all",
    "upsert",
    "upsert_all",
    "touch",
    "increment!",
    "decrement!",
    "toggle!",
    "write_attribute",
];

/// Is `method` one of the [`AR_MUTATORS`]?
fn is_ar_mutator(method: &str) -> bool {
    AR_MUTATORS.contains(&method)
}

/// Is `name` a valid Ruby attribute identifier — `[A-Za-z_][A-Za-z0-9_]*`?
/// Distinguishes attribute reads/setters (`name`, `name=`) from operator
/// methods (`==`, `<=`, `[]=`, `+`, …), so an operator self-send never
/// becomes a `reads`/`writes` entry. A setter is recognised by stripping the
/// trailing `=` and checking the base with this — `==` strips to `=` (not an
/// ident → not a write), `state=` strips to `state` (ident → a write).
fn is_attr_ident(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Best-effort label for a call receiver, used for the `calls` capture
/// (`"<receiver>.<method>"`). A bare call (`None`) and an explicit `self`
/// both render as `"self"`. A relation/local receiver renders as its name; a
/// constant as its dotted path; an unresolvable receiver as `"<expr>"`.
fn receiver_label(recv: Option<&Node>) -> String {
    match recv {
        None | Some(Node::Self_(_)) => "self".to_string(),
        Some(node @ Node::Const(_)) => {
            const_to_dotted(node).unwrap_or_else(|| "<const>".to_string())
        }
        Some(Node::Lvar(l)) => l.name.clone(),
        Some(Node::Ivar(i)) => i.name.clone(),
        // `order.update` / `self.order.update` — the immediate receiver is a
        // bare or self-rooted send naming the relation/attribute.
        Some(Node::Send(inner))
            if inner.recv.is_none() || matches!(inner.recv.as_deref(), Some(Node::Self_(_))) =>
        {
            inner.method_name.clone()
        }
        _ => "<expr>".to_string(),
    }
}

/// Extract the exception type-name from a `raise <arg>` argument.
///
/// - `raise UserError` → `"UserError"` (`Node::Const`).
/// - `raise UserError.new("msg")` → `"UserError"` (`Node::Send` with recv=Const).
/// - `raise UserError, "msg"` → handled by caller via `args.first()`.
/// - `raise foo` (variable) → `None` (can't statically resolve).
fn exception_type_name(arg: &Node) -> Option<String> {
    match arg {
        Node::Const(_) => const_to_dotted(arg),
        Node::Send(s) if s.method_name == "new" => s.recv.as_deref().and_then(const_to_dotted),
        _ => None,
    }
}

/// Render a constant-chain node to a dotted string. (Re-implemented
/// here because `parse.rs::const_to_string` is module-local; both
/// functions agree on the format.)
fn const_to_dotted(node: &Node) -> Option<String> {
    let Node::Const(c) = node else { return None };
    let suffix = c.name.clone();
    match c.scope.as_deref() {
        Some(Node::Cbase(_)) => Some(format!("::{suffix}")),
        Some(inner) => const_to_dotted(inner).map(|p| format!("{p}::{suffix}")),
        None => Some(suffix),
    }
}

/// If `s` is a method call whose immediate receiver is a known relation
/// name, return the relation. The receiver can be:
///
/// - `self.<rel>` — `s.recv == Self_`, but then we'd be reading
///   `<rel>` on self, not traversing — handled by the `reads` arm.
/// - `<rel>` bare (the recv is `Node::Send { method_name: "rel", recv: None }`
///   OR `Node::Lvar("rel")`) — that's the traversal entry.
fn traversed_relation(s: &lib_ruby_parser::nodes::Send, known: &[String]) -> Option<String> {
    let recv = s.recv.as_deref()?;
    // `<rel>.each` — recv is a bare send with method == rel name and no
    // further receiver.
    if let Node::Send(inner) = recv {
        if inner.recv.is_none() && known.iter().any(|r| r == &inner.method_name) {
            return Some(inner.method_name.clone());
        }
    }
    // `self.<rel>.each` — recv is `self.<rel>`, i.e. a Send with recv=Self.
    if let Node::Send(inner) = recv {
        if matches!(inner.recv.as_deref(), Some(Node::Self_(_)))
            && known.iter().any(|r| r == &inner.method_name)
        {
            return Some(inner.method_name.clone());
        }
    }
    None
}

/// `for r in <expr>` — does `<expr>` name a known relation?
fn node_relation_name(node: &Node, known: &[String]) -> Option<String> {
    match node {
        Node::Send(s) if s.recv.is_none() && known.iter().any(|r| r == &s.method_name) => {
            Some(s.method_name.clone())
        }
        Node::Send(s)
            if matches!(s.recv.as_deref(), Some(Node::Self_(_)))
                && known.iter().any(|r| r == &s.method_name) =>
        {
            Some(s.method_name.clone())
        }
        _ => None,
    }
}

fn dedup_in_place<T: Ord + Clone>(v: &mut Vec<T>) {
    v.sort();
    v.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract_from_source;
    use lib_ruby_parser::{Parser, ParserOptions};

    fn class_functions(src: &str) -> Vec<Function> {
        let classes = extract_from_source(src);
        assert_eq!(classes.len(), 1, "expected exactly one class");
        // The test entry uses the public extract path which still has
        // empty functions — call extract_functions_from_body directly
        // on the AST. Re-parse for the test fixture.
        let parser = Parser::new(src.as_bytes().to_vec(), ParserOptions::default());
        let ast = parser.do_parse().ast.expect("parse");
        let class = find_first_class(&ast).expect("class");
        extract_functions_from_body(class.body.as_deref(), &classes[0].declarations).0
    }

    fn find_first_class(node: &Node) -> Option<&lib_ruby_parser::nodes::Class> {
        match node {
            Node::Class(c) => Some(c),
            Node::Module(m) => m.body.as_deref().and_then(find_first_class),
            Node::Begin(b) => b.statements.iter().find_map(find_first_class),
            _ => None,
        }
    }

    #[test]
    fn bare_private_marker_hides_subsequent_defs() {
        // The Rails controller pattern: public actions, then a bare `private`,
        // then non-routable helpers (`set_project`, `authorize`). Only the
        // public actions must be harvested (codex #42).
        let funcs = class_functions(
            r#"
class NodesController
  def show
  end
  def create
  end

  private

  def set_project
  end
  def authorize
  end
end
"#,
        );
        let names: Vec<&str> = funcs.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["show", "create"]);
    }

    #[test]
    fn public_marker_flips_visibility_back() {
        let funcs = class_functions(
            r#"
class M
  private
  def helper
  end
  public
  def action
  end
end
"#,
        );
        let names: Vec<&str> = funcs.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["action"]);
    }

    #[test]
    fn private_symbol_form_hides_named_method() {
        // `private :set_project` after the def — retroactive by name.
        let funcs = class_functions(
            r#"
class M
  def show
  end
  def set_project
  end
  private :set_project
end
"#,
        );
        let names: Vec<&str> = funcs.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["show"]);
    }

    #[test]
    fn private_inline_def_form_is_hidden() {
        let funcs = class_functions(
            r#"
class M
  def show
  end
  private def set_project
  end
end
"#,
        );
        let names: Vec<&str> = funcs.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["show"]);
    }

    #[test]
    fn visibility_is_transparent_across_begin_end() {
        // A bare `private` inside a nested `begin/end` still governs later
        // class-body siblings (begin/end is transparent in Ruby) — the def after
        // it must NOT be harvested (Bugbot: visibility reset per Begin).
        let funcs = class_functions(
            r#"
class M
  def show
  end
  begin
    private
  end
  def helper
  end
end
"#,
        );
        let names: Vec<&str> = funcs.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["show"]);
    }

    #[test]
    fn public_symbol_republishes_a_privatised_method() {
        // `private :show` then `public :show` — Ruby's final visibility is
        // public, so `show` must be harvested (codex P2 / Bugbot: append-only
        // non_public dropped it).
        let funcs = class_functions(
            r#"
class M
  def show
  end
  private :show
  public :show
end
"#,
        );
        let names: Vec<&str> = funcs.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["show"]);
    }

    #[test]
    fn def_emits_function_name() {
        let funcs = class_functions(
            r#"
class M
  def compute_total
  end
  def status
  end
end
"#,
        );
        let names: Vec<&str> = funcs.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["compute_total", "status"]);
    }

    #[test]
    fn raise_const_captures_exception_type() {
        let funcs = class_functions(
            r#"
class M
  def must_be_valid
    raise ::ActiveRecord::RecordInvalid
  end
end
"#,
        );
        assert_eq!(funcs[0].raises, vec!["::ActiveRecord::RecordInvalid"]);
    }

    #[test]
    fn raise_new_captures_exception_type() {
        let funcs = class_functions(
            r#"
class M
  def fail!
    raise UserError.new("oops")
  end
end
"#,
        );
        assert_eq!(funcs[0].raises, vec!["UserError"]);
    }

    #[test]
    fn raise_variable_skipped() {
        let funcs = class_functions(
            r#"
class M
  def relay(err)
    raise err
  end
end
"#,
        );
        assert!(
            funcs[0].raises.is_empty(),
            "variable raise must not be captured"
        );
    }

    #[test]
    fn self_dot_attribute_emits_read() {
        let funcs = class_functions(
            r#"
class M
  def fmt
    self.subject
  end
end
"#,
        );
        assert_eq!(funcs[0].reads, vec!["subject"]);
    }

    #[test]
    fn association_walk_emits_traversal() {
        let funcs = class_functions(
            r#"
class M
  belongs_to :project
  has_many :time_entries

  def total_hours
    time_entries.each { |te| te.hours }
  end

  def project_name
    self.project.name
  end
end
"#,
        );
        let total_hours = funcs.iter().find(|f| f.name == "total_hours").unwrap();
        assert!(
            total_hours.traverses.contains(&"time_entries".to_string()),
            "expected time_entries traversal; got {:?}",
            total_hours.traverses,
        );
        let proj_name = funcs.iter().find(|f| f.name == "project_name").unwrap();
        assert!(
            proj_name.traverses.contains(&"project".to_string()),
            "expected project traversal; got {:?}",
            proj_name.traverses,
        );
    }

    #[test]
    fn for_loop_emits_traversal() {
        let funcs = class_functions(
            r#"
class M
  has_many :time_entries

  def report
    for t in time_entries
      t.hours
    end
  end
end
"#,
        );
        assert!(
            funcs[0].traverses.contains(&"time_entries".to_string()),
            "for-loop must extract traversal; got {:?}",
            funcs[0].traverses,
        );
    }

    #[test]
    fn unrelated_send_does_not_emit_traversal() {
        let funcs = class_functions(
            r#"
class M
  def calc
    something_unknown.each { |x| x }
  end
end
"#,
        );
        assert!(
            funcs[0].traverses.is_empty(),
            "no traversal expected for unknown method; got {:?}",
            funcs[0].traverses,
        );
    }

    #[test]
    fn raises_in_rescue_arm_captured() {
        let funcs = class_functions(
            r#"
class M
  def safe
    do_work
  rescue StandardError
    raise UserError
  end
end
"#,
        );
        assert!(
            funcs[0].raises.contains(&"UserError".to_string()),
            "rescue-arm raise must be captured; got {:?}",
            funcs[0].raises,
        );
    }

    #[test]
    fn raise_in_postfix_unless_captures_exception_type() {
        // `raise X unless cond` — the postfix `unless` modifier (`Node::IfMod`
        // with `if_false` set). Root-caused from openproject's WorkPackage
        // fixture: `raise ActiveRecord::RecordInvalid unless status` was
        // invisible to the harvest because `walk_method_body` had no arm for
        // `Node::IfMod`, only `Node::If` (block form).
        let funcs = class_functions(
            r#"
class M
  def compute_total_hours
    raise ActiveRecord::RecordInvalid unless status
  end
end
"#,
        );
        assert!(
            funcs[0]
                .raises
                .contains(&"ActiveRecord::RecordInvalid".to_string()),
            "postfix-unless raise must be captured; got {:?}",
            funcs[0].raises,
        );
    }

    #[test]
    fn self_assignment_emits_write_not_read() {
        let funcs = class_functions(
            r#"
class M
  def post
    self.state = "posted"
  end
end
"#,
        );
        assert_eq!(funcs[0].writes, vec!["state"]);
        // The `state=` setter must NOT leak into `reads` as `"state="`.
        assert!(
            funcs[0].reads.is_empty(),
            "a write must not be recorded as a read; got reads {:?}",
            funcs[0].reads,
        );
    }

    #[test]
    fn write_and_read_coexist() {
        let funcs = class_functions(
            r#"
class M
  def recompute
    self.total = self.subtotal
  end
end
"#,
        );
        assert_eq!(funcs[0].writes, vec!["total"]);
        assert_eq!(funcs[0].reads, vec!["subtotal"]);
    }

    #[test]
    fn ivar_assignment_is_not_a_write() {
        // `@x = …` is local memoization, not an AR attribute write.
        let funcs = class_functions(
            r#"
class M
  def memo
    @cache = expensive
  end
end
"#,
        );
        assert!(
            funcs[0].writes.is_empty(),
            "instance-var assignment must not be a field write; got {:?}",
            funcs[0].writes,
        );
    }

    #[test]
    fn bare_and_self_mutator_emit_self_call() {
        let funcs = class_functions(
            r#"
class M
  def persist
    save
  end
  def persist_bang
    self.save!
  end
end
"#,
        );
        let persist = funcs.iter().find(|f| f.name == "persist").unwrap();
        assert!(
            persist.calls.contains(&"self.save".to_string()),
            "bare `save` must be a self-call; got {:?}",
            persist.calls,
        );
        let persist_bang = funcs.iter().find(|f| f.name == "persist_bang").unwrap();
        assert!(
            persist_bang.calls.contains(&"self.save!".to_string()),
            "`self.save!` must be a self-call; got {:?}",
            persist_bang.calls,
        );
    }

    #[test]
    fn receiver_mutator_emits_call() {
        let funcs = class_functions(
            r#"
class M
  def touch_order(order)
    order.update(state: "x")
    User.create!(name: "y")
  end
end
"#,
        );
        assert!(
            funcs[0].calls.contains(&"order.update".to_string()),
            "local-receiver mutator must be captured; got {:?}",
            funcs[0].calls,
        );
        assert!(
            funcs[0].calls.contains(&"User.create!".to_string()),
            "const-receiver mutator must be captured; got {:?}",
            funcs[0].calls,
        );
    }

    #[test]
    fn non_mutator_call_not_captured() {
        let funcs = class_functions(
            r#"
class M
  def compute
    helper.format(value)
  end
end
"#,
        );
        assert!(
            funcs[0].calls.is_empty(),
            "non-mutator calls must not be captured; got {:?}",
            funcs[0].calls,
        );
    }

    #[test]
    fn operator_self_send_is_neither_write_nor_read() {
        // `self == other` / `self <= other` are comparison operators whose
        // method names end in `=` — they must NOT strip to a bogus write of a
        // field named `=`/`<` (codex P2), nor be recorded as reads.
        let funcs = class_functions(
            r#"
class M
  def ==(other)
    self.id == other.id
  end
  def le(other)
    self <= other
  end
end
"#,
        );
        let eq = funcs.iter().find(|f| f.name == "==").unwrap();
        assert!(
            eq.writes.is_empty(),
            "operator method must not produce a write; got {:?}",
            eq.writes,
        );
        // `self.id` IS a valid attribute read; the `==` operator send is not.
        assert_eq!(eq.reads, vec!["id"]);
        let le = funcs.iter().find(|f| f.name == "le").unwrap();
        assert!(
            le.writes.is_empty() && le.reads.is_empty(),
            "`self <= other` is neither write nor read; got writes {:?} reads {:?}",
            le.writes,
            le.reads,
        );
    }

    #[test]
    fn postfix_if_write_is_captured() {
        // `self.<field> = … if cond` — the postfix `if` modifier (`Node::IfMod`
        // with `if_true` set), the mirror case of the postfix `unless` form.
        let funcs = class_functions(
            r#"
class M
  def reset_total
    self.total = 0 if reset
  end
end
"#,
        );
        assert_eq!(funcs[0].writes, vec!["total"]);
    }

    #[test]
    fn private_defs_land_in_helpers_with_body_facts() {
        // Rails lifecycle callbacks conventionally target private methods
        // (OGAR F17 body triage needs their body facts). The visibility
        // split must keep them OUT of the action vec but IN helpers, with
        // the same body walk applied.
        let src = r#"
class M
  def visible
    self.a = 1
  end
  private
  def hook_me
    raise Foo unless ok
    self.b = 2
  end
end
"#;
        let classes = extract_from_source(src);
        let parser = Parser::new(src.as_bytes().to_vec(), ParserOptions::default());
        let ast = parser.do_parse().ast.expect("parse");
        let class = find_first_class(&ast).expect("class");
        let (public_fns, helper_fns) =
            extract_functions_from_body(class.body.as_deref(), &classes[0].declarations);
        assert_eq!(public_fns.len(), 1, "only the public def is an action");
        assert_eq!(helper_fns.len(), 1, "the private def lands in helpers");
        let h = &helper_fns[0];
        assert_eq!(h.name, "hook_me");
        assert!(
            h.raises.contains(&"Foo".to_string()),
            "postfix-unless raise walked in helper; got {:?}",
            h.raises
        );
        assert!(
            h.writes.contains(&"b".to_string()),
            "setter write walked in helper; got {:?}",
            h.writes
        );
    }

    #[test]
    fn j1_write_if_blank_is_guarded_default_not_normalize() {
        // The J1 split: a write guarded by a blank/nil test on the SAME field
        // is a schema-default (`guarded_writes`); an unconditional self-write
        // is a `normalizes` transform (writes only). See
        // .claude/knowledge/fuzzy-recipe-codebook.md §5 (J1).
        let default_form = class_functions(
            r#"
class M
  def set_default
    self.state = "new" if self.state.blank?
  end
end
"#,
        );
        assert_eq!(default_form[0].writes, vec!["state"]);
        assert_eq!(
            default_form[0].guarded_writes,
            vec!["state"],
            "write-if-blank must be a guarded (default) write"
        );

        let unless_form = class_functions(
            r#"
class M
  def set_default
    self.state = "new" unless self.state.present?
  end
end
"#,
        );
        assert_eq!(
            unless_form[0].guarded_writes,
            vec!["state"],
            "`unless present?` is the same guard as `if blank?`"
        );

        let normalize_form = class_functions(
            r#"
class M
  def tidy
    self.path = sanitize(self.path)
  end
end
"#,
        );
        assert_eq!(normalize_form[0].writes, vec!["path"]);
        assert!(
            normalize_form[0].guarded_writes.is_empty(),
            "an unconditional transform is a normalize, not a default; got {:?}",
            normalize_form[0].guarded_writes
        );
    }

    #[test]
    fn or_asgn_self_attr_emits_guarded_and_plain_write() {
        // `self.x ||= v` is the most common default idiom (#45-audit blind
        // spot): semantically the same as `self.x = v if self.x.blank?`, so
        // it must land in BOTH `writes` and `guarded_writes` (subset
        // invariant), not neither.
        let funcs = class_functions(
            r#"
class M
  def set_default
    self.state ||= "new"
  end
end
"#,
        );
        assert_eq!(funcs[0].writes, vec!["state"]);
        assert_eq!(
            funcs[0].guarded_writes,
            vec!["state"],
            "`self.x ||= v` must be recorded as a guarded (default) write"
        );
    }

    #[test]
    fn or_asgn_local_var_emits_nothing() {
        // `x ||= v` (a local variable, no `self.` receiver) is not an AR
        // attribute write — consistent with `attr_of_self` / `is_attr_ident`
        // elsewhere (e.g. `@x = …` is likewise not a write).
        let funcs = class_functions(
            r#"
class M
  def compute
    x ||= 1
    x
  end
end
"#,
        );
        assert!(
            funcs[0].writes.is_empty(),
            "local-var or-asgn must not be a field write; got {:?}",
            funcs[0].writes
        );
        assert!(
            funcs[0].guarded_writes.is_empty(),
            "local-var or-asgn must not be a guarded write; got {:?}",
            funcs[0].guarded_writes
        );
    }

    #[test]
    fn or_asgn_ivar_emits_nothing() {
        // `@x ||= v` (ivar memoization) is not an AR attribute write —
        // pins the doc-comment claim explicitly.
        let funcs = class_functions(
            r#"
class M
  def memo
    @cache ||= expensive
  end
end
"#,
        );
        assert!(funcs[0].writes.is_empty(), "ivar or-asgn must not be a field write");
        assert!(funcs[0].guarded_writes.is_empty());
    }

    #[test]
    fn or_asgn_rhs_facts_are_still_walked() {
        // Facts inside the RHS of `self.a ||= …` must not be lost: the value
        // expression is walked like any other body statement.
        let funcs = class_functions(
            r#"
class M
  def default_name
    self.name ||= build_from(self.slug)
  end
end
"#,
        );
        assert_eq!(funcs[0].writes, vec!["name"]);
        assert_eq!(funcs[0].guarded_writes, vec!["name"]);
        assert!(
            funcs[0].reads.contains(&"slug".to_string()),
            "RHS read of self.slug must be captured; got {:?}",
            funcs[0].reads
        );
    }

    #[test]
    fn and_asgn_self_attr_emits_plain_write_not_guarded() {
        // `self.x &&= v` is a write but NOT a J1 guarded default (it assigns
        // only when the field is PRESENT — the mirror idiom).
        let funcs = class_functions(
            r#"
class M
  def normalize
    self.email &&= email.strip
  end
end
"#,
        );
        assert_eq!(funcs[0].writes, vec!["email"]);
        assert!(
            funcs[0].guarded_writes.is_empty(),
            "&&= must not be guarded_writes (present-guard, not J1); got {:?}",
            funcs[0].guarded_writes
        );
    }

    #[test]
    fn op_asgn_self_attr_emits_write_and_read() {
        // `self.x += v` reads the current value to compute the new one — a
        // read-modify-write, NOT a guarded default (no blank/nil test), so it
        // must be `writes` + `reads`, never `guarded_writes`.
        let funcs = class_functions(
            r#"
class M
  def bump
    self.count += 1
  end
end
"#,
        );
        assert_eq!(funcs[0].writes, vec!["count"]);
        assert_eq!(funcs[0].reads, vec!["count"]);
        assert!(
            funcs[0].guarded_writes.is_empty(),
            "op-asgn is a read-modify-write, not a guarded default; got {:?}",
            funcs[0].guarded_writes
        );
    }
}
