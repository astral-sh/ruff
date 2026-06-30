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
//! - Op-assign writes (`self.x += 1`, `self.x ||= y`) — only the plain
//!   `self.x = …` setter form is recorded as a write today.
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
) -> Vec<Function> {
    let Some(body) = body else {
        return Vec::new();
    };
    let known_relations = collect_known_relations(declarations);
    let mut out = Vec::new();
    walk_class_body_for_defs(body, &known_relations, &mut out);
    out
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
/// blocks; for each `Node::Def` encountered, extract one Function.
fn walk_class_body_for_defs(
    node: &Node,
    known_relations: &[String],
    out: &mut Vec<Function>,
) {
    match node {
        Node::Begin(b) => {
            for stmt in &b.statements {
                walk_class_body_for_defs(stmt, known_relations, out);
            }
        }
        Node::Def(d) => {
            let mut func = Function {
                name: d.name.clone(),
                reads: Vec::new(),
                raises: Vec::new(),
                traverses: Vec::new(),
                writes: Vec::new(),
                calls: Vec::new(),
            };
            if let Some(fn_body) = d.body.as_deref() {
                walk_method_body(fn_body, known_relations, &mut func);
            }
            // Dedupe per-function reads / raises / traverses / writes /
            // calls (a method that calls `raise UserError` twice should
            // produce one `raises` triple, not two — `expand()` dedupes by
            // (s, p, o) anyway, but keeping the IR tight reduces churn).
            dedup_in_place(&mut func.reads);
            dedup_in_place(&mut func.raises);
            dedup_in_place(&mut func.traverses);
            dedup_in_place(&mut func.writes);
            dedup_in_place(&mut func.calls);
            out.push(func);
        }
        Node::Block(blk) => {
            // `class_methods do … end` / `included do … end` blocks
            // wrap `def` nodes whose methods are class-level (or
            // instance-level via include). The walker treats them as
            // siblings of regular `def` for now — D-AR-3.6 will split
            // class-method discovery off.
            if let Some(body) = blk.body.as_deref() {
                walk_class_body_for_defs(body, known_relations, out);
            }
        }
        _ => {}
    }
}

/// Walk one method body, populating `func.reads` / `raises` / `traverses`.
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
            if let Some(field) = s.method_name.strip_suffix('=') {
                // `self.<field> = …` — the setter call: a write of `<field>`.
                func.writes.push(field.to_string());
            } else if is_ar_mutator(&s.method_name) {
                // `self.save` / `self.update(...)` — lifecycle mutator on self.
                func.calls.push(format!("self.{}", s.method_name));
            } else {
                // `self.<field>` — a plain attribute read.
                func.reads.push(s.method_name.clone());
            }
            // Recurse into args (the RHS of a write, or call/read args, may
            // themselves contain a raise/read/write/call).
            for arg in &s.args {
                walk_method_body(arg, known_relations, func);
            }
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
            if inner.recv.is_none()
                || matches!(inner.recv.as_deref(), Some(Node::Self_(_))) =>
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
        Node::Send(s) if s.method_name == "new" => {
            s.recv.as_deref().and_then(const_to_dotted)
        }
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
        Some(inner) => {
            const_to_dotted(inner).map(|p| format!("{p}::{suffix}"))
        }
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
        extract_functions_from_body(class.body.as_deref(), &classes[0].declarations)
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
        assert!(funcs[0].raises.is_empty(), "variable raise must not be captured");
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
}
