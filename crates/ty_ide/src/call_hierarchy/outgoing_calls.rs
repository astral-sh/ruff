use crate::call_hierarchy::{CalleeLeaf, callee_leaf, resolve_callee};
use crate::goto::find_goto_target;
use crate::{CallHierarchyItem, Db};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::token::Tokens;
use ruff_python_ast::visitor::source_order::{
    walk_arguments, walk_decorator, walk_expr, walk_parameters, walk_type_params,
};
use ruff_python_ast::{
    self as ast, AnyNodeRef,
    visitor::source_order::{SourceOrderVisitor, TraversalSignal, walk_body},
};
use ruff_text_size::{Ranged, TextRange, TextSize};
use rustc_hash::FxHashMap;
use ty_python_core::definition::DefinitionKind;
use ty_python_semantic::{ImportAliasResolution, SemanticModel};

/// Find every function/method/class that the symbol at `offset` calls.
///
/// "Calls" means the callables evaluated when the symbol's own definition
/// statement runs — for a function: its decorators, type parameters, parameter
/// defaults and annotations, return-type annotation, and direct calls in the
/// body; for a class: its decorators, type parameters, base-class expressions
/// and keyword arguments, and direct calls in the class body (including
/// decorators / parameter defaults on its methods, which are evaluated at
/// class-body time). Nested function / class / lambda *bodies* are
/// deliberately not transited: each nested callable is its own
/// `CallHierarchyItem` with its own outgoing edges, expandable separately by
/// the LSP client.
pub fn outgoing_calls(db: &dyn Db, file: File, offset: TextSize) -> Vec<OutgoingCall> {
    let module = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let Some(goto_target) = find_goto_target(&model, &module, offset) else {
        return Vec::new();
    };
    let Some(definitions) = goto_target
        .definitions(&model, ImportAliasResolution::ResolveAliases)
        .and_then(|d| d.goto_declaration(&model, &goto_target))
    else {
        return Vec::new();
    };

    // Use a stable group key so multiple call sites to the same callee fold into
    // one outgoing entry, and so output order is deterministic across runs.
    let mut groups: FxHashMap<CalleeKey, (CallHierarchyItem, Vec<TextRange>)> =
        FxHashMap::default();

    for resolved in &definitions {
        let Some(def) = resolved.definition() else {
            continue;
        };
        let def_file = def.file(db);
        let parsed = parsed_module(db, def_file).load(db);

        let body_model = SemanticModel::new(db, def_file);
        let mut finder = OutgoingCallsFinder {
            db,
            model: &body_model,
            tokens: parsed.tokens(),
            groups: &mut groups,
            ancestors: Vec::new(),
            seen_for_this_call: rustc_hash::FxHashSet::default(),
        };

        // Walk the queried symbol's signature parts (everything evaluated when
        // its `def`/`class` statement runs) and then its body. Inside the body
        // the visitor stops at nested callables — see `OutgoingCallsFinder`.
        match def.kind(db) {
            DefinitionKind::Function(fn_ref) => {
                let func = fn_ref.node(&parsed);
                walk_callable_signature(
                    &mut finder,
                    &func.decorator_list,
                    func.type_params.as_deref(),
                    Some(&func.parameters),
                    func.returns.as_deref(),
                );
                walk_body(&mut finder, &func.body);
            }
            DefinitionKind::Class(class_ref) => {
                let class = class_ref.node(&parsed);
                walk_class_signature(
                    &mut finder,
                    &class.decorator_list,
                    class.type_params.as_deref(),
                    class.arguments.as_deref(),
                );
                walk_body(&mut finder, &class.body);
            }
            _ => continue,
        }
    }

    let mut results: Vec<_> = groups
        .into_values()
        .map(|(to, from_ranges)| OutgoingCall { to, from_ranges })
        .collect();
    // Stable order: by callee file path string, then range.
    results.sort_by(|a, b| {
        let a_path = a.to.file.path(db).as_str();
        let b_path = b.to.file.path(db).as_str();
        a_path.cmp(b_path).then_with(|| {
            a.to.selection_range
                .start()
                .cmp(&b.to.selection_range.start())
        })
    });
    results
}

#[derive(Debug, Clone)]
pub struct OutgoingCall {
    /// The function/method/class that is being called.
    pub to: CallHierarchyItem,
    /// Call-site ranges inside the prepared item's body.
    pub from_ranges: Vec<TextRange>,
}

/// AST visitor that, for a single function/class body, records every callee.
struct OutgoingCallsFinder<'a, 'db> {
    db: &'db dyn Db,
    model: &'a SemanticModel<'db>,
    tokens: &'a Tokens,
    groups: &'a mut FxHashMap<CalleeKey, (CallHierarchyItem, Vec<TextRange>)>,
    ancestors: Vec<AnyNodeRef<'a>>,
    /// Reused across `record_callee` invocations: cleared at the top of each
    /// call instead of allocated fresh. Carries the `(file, selection_range)`
    /// dedup keys for one call site's resolved-definitions iteration.
    seen_for_this_call: rustc_hash::FxHashSet<(File, TextRange)>,
}

impl<'a> OutgoingCallsFinder<'a, '_> {
    fn record_callee(&mut self, leaf: CalleeLeaf<'a>) {
        let Some((goto_target, call_site_range)) =
            resolve_callee(self.model, self.tokens, &self.ancestors, leaf)
        else {
            return;
        };
        let Some(definitions) = goto_target
            .definitions(self.model, ImportAliasResolution::ResolveAliases)
            .and_then(|d| d.goto_declaration(self.model, &goto_target))
        else {
            return;
        };

        // A single call site can resolve to multiple `ResolvedDefinition`s
        // pointing at the same logical callee (overload chains, co-definitions,
        // import alias + underlying). Deduplicate by callee key so this call
        // site contributes exactly one range per distinct callee.
        //
        // We compute the dedup key (`(file, selection_range)`) up-front and
        // only construct the full `CallHierarchyItem` when inserting a new
        // entry, so callees hit repeatedly through the body don't pay repeated
        // name allocations and range computations.
        self.seen_for_this_call.clear();
        for resolved in &definitions {
            let Some(def) = resolved.definition() else {
                continue;
            };
            // Only Function / Class kinds become items; bail early so we
            // don't pay a parsed_module lookup for a kind that will be
            // dropped anyway.
            match def.kind(self.db) {
                DefinitionKind::Function(_) | DefinitionKind::Class(_) => {}
                _ => continue,
            }
            let def_file = def.file(self.db);
            let module_ref = parsed_module(self.db, def_file).load(self.db);
            let selection_range = def.focus_range(self.db, &module_ref).range();
            if !self.seen_for_this_call.insert((def_file, selection_range)) {
                continue;
            }
            let key = CalleeKey {
                file: def_file,
                selection_range,
            };
            if let Some((_, ranges)) = self.groups.get_mut(&key) {
                ranges.push(call_site_range);
            } else if let Some(item) =
                CallHierarchyItem::from_definition(self.db, resolved, &module_ref)
            {
                self.groups.insert(key, (item, vec![call_site_range]));
            }
        }
    }
}

impl<'a> SourceOrderVisitor<'a> for OutgoingCallsFinder<'a, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        self.ancestors.push(node);

        match node {
            AnyNodeRef::ExprCall(call) => {
                if let Some(leaf) = callee_leaf(&call.func) {
                    self.record_callee(leaf);
                }
            }
            AnyNodeRef::Decorator(decorator) => {
                // A bare `@foo` decorator with no parens is a runtime call. If
                // the user wrote `@foo()` we'll pick it up via the `ExprCall`
                // arm above instead.
                if let Some(leaf) = callee_leaf(&decorator.expression) {
                    self.record_callee(leaf);
                }
            }
            // Nested callables: walk only the parts evaluated when the
            // surrounding scope runs (decorators, defaults, bases, ...). The
            // body belongs to the nested item itself and is reached by
            // expanding that item separately in the call hierarchy tree.
            AnyNodeRef::StmtFunctionDef(func) => {
                walk_callable_signature(
                    self,
                    &func.decorator_list,
                    func.type_params.as_deref(),
                    Some(&func.parameters),
                    func.returns.as_deref(),
                );
                return TraversalSignal::Skip;
            }
            AnyNodeRef::StmtClassDef(class) => {
                walk_class_signature(
                    self,
                    &class.decorator_list,
                    class.type_params.as_deref(),
                    class.arguments.as_deref(),
                );
                return TraversalSignal::Skip;
            }
            AnyNodeRef::ExprLambda(lambda) => {
                walk_callable_signature(self, &[], None, lambda.parameters.as_deref(), None);
                return TraversalSignal::Skip;
            }
            _ => {}
        }

        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, node: AnyNodeRef<'a>) {
        debug_assert_eq!(self.ancestors.last(), Some(&node));
        self.ancestors.pop();
    }
}

#[derive(PartialEq, Eq, Hash)]
struct CalleeKey {
    file: File,
    selection_range: TextRange,
}

/// Visit the definition-time parts of a function / lambda — everything
/// evaluated when the `def` (or `lambda` expression) is reached at runtime
/// in the *enclosing* scope: decorators, type parameters, parameter defaults
/// and annotations, return-type annotation.
fn walk_callable_signature<'a, V>(
    visitor: &mut V,
    decorator_list: &'a [ast::Decorator],
    type_params: Option<&'a ast::TypeParams>,
    parameters: Option<&'a ast::Parameters>,
    returns: Option<&'a ast::Expr>,
) where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    for decorator in decorator_list {
        walk_decorator(visitor, decorator);
    }
    if let Some(type_params) = type_params {
        walk_type_params(visitor, type_params);
    }
    if let Some(parameters) = parameters {
        walk_parameters(visitor, parameters);
    }
    if let Some(returns) = returns {
        walk_expr(visitor, returns);
    }
}

/// Visit the definition-time parts of a class statement: decorators, type
/// parameters, base classes / keyword arguments / metaclass. The body is
/// handled separately so the caller can decide whether to walk it.
fn walk_class_signature<'a, V>(
    visitor: &mut V,
    decorator_list: &'a [ast::Decorator],
    type_params: Option<&'a ast::TypeParams>,
    arguments: Option<&'a ast::Arguments>,
) where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    for decorator in decorator_list {
        walk_decorator(visitor, decorator);
    }
    if let Some(type_params) = type_params {
        walk_type_params(visitor, type_params);
    }
    if let Some(arguments) = arguments {
        walk_arguments(visitor, arguments);
    }
}

#[cfg(test)]
mod tests {
    use ty_project::Db;

    use crate::{
        OutgoingCall,
        call_hierarchy::tests::snapshot_item,
        outgoing_calls,
        tests::{CursorTest, cursor_test},
    };

    fn snapshot_outgoing(db: &dyn Db, calls: &[OutgoingCall]) -> String {
        calls
            .iter()
            .map(|call| {
                let head = snapshot_item(db, &call.to);
                let ranges: Vec<String> = call
                    .from_ranges
                    .iter()
                    .map(|r| format!("  call @ {}..{}", r.start().to_usize(), r.end().to_usize()))
                    .collect();
                format!("{head}\n{}", ranges.join("\n"))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    impl CursorTest {
        fn outgoing(&self) -> Vec<OutgoingCall> {
            let Some(items) = self.prepare_calls() else {
                return vec![];
            };
            let item = &items[0];
            outgoing_calls(&self.db, item.file, item.selection_range.start())
        }
    }

    #[test]
    fn outgoing_direct_call() {
        let test = cursor_test(
            r#"
            def helper():
                pass

            def f<CURSOR>oo():
                helper()
            "#,
        );
        insta::assert_snapshot!(snapshot_outgoing(&test.db, &test.outgoing()), @"
        /main.py:5:11 helper (Function)
          call @ 40..46
        ");
    }

    #[test]
    fn outgoing_attribute_call() {
        let test = cursor_test(
            r#"
            class C:
                def m(self):
                    pass

            def f<CURSOR>oo(c: C):
                c.m()
            "#,
        );
        insta::assert_snapshot!(snapshot_outgoing(&test.db, &test.outgoing()), @"
        /main.py:18:19 m (Method)
          call @ 62..63
        ");
    }

    #[test]
    fn outgoing_constructor_call() {
        let test = cursor_test(
            r#"
            class C:
                pass

            def f<CURSOR>oo():
                C()
            "#,
        );
        insta::assert_snapshot!(snapshot_outgoing(&test.db, &test.outgoing()), @"
        /main.py:7:8 C (Class)
          call @ 35..36
        ");
    }

    #[test]
    fn outgoing_multiple_calls_to_same_callee() {
        let test = cursor_test(
            r#"
            def helper():
                pass

            def f<CURSOR>oo():
                helper()
                helper()
            "#,
        );
        let outgoing = test.outgoing();
        assert_eq!(outgoing.len(), 1, "expected one callee group");
        assert_eq!(outgoing[0].from_ranges.len(), 2, "expected two call sites");
    }

    #[test]
    fn outgoing_class_excludes_method_bodies() {
        // Calls inside method bodies belong to the method, not the class. They
        // are reachable from a separate `outgoingCalls` query on the method.
        let test = cursor_test(
            r#"
            def helper_init():
                pass

            def helper_other():
                pass

            class Cl<CURSOR>s:
                def __init__(self):
                    helper_init()

                def other(self):
                    helper_other()
            "#,
        );
        let names: Vec<_> = test
            .outgoing()
            .into_iter()
            .map(|c| c.to.name.as_str().to_string())
            .collect();
        assert!(
            !names.contains(&"helper_init".to_string()),
            "method body call should NOT appear under class; got: {names:?}"
        );
        assert!(
            !names.contains(&"helper_other".to_string()),
            "method body call should NOT appear under class; got: {names:?}"
        );
    }

    #[test]
    fn outgoing_class_includes_decorators_bases_and_class_body() {
        // Everything evaluated when the `class` statement runs.
        let test = cursor_test(
            r#"
            def cls_deco(cls):
                return cls

            def base_factory():
                return object

            def class_body_helper():
                return 1

            def method_deco(fn):
                return fn

            def default_factory():
                return None

            @cls_deco
            class C<CURSOR>ls(base_factory()):
                attr = class_body_helper()

                @method_deco
                def m(self, x=default_factory()):
                    pass
            "#,
        );
        let names: Vec<_> = test
            .outgoing()
            .into_iter()
            .map(|c| c.to.name.as_str().to_string())
            .collect();
        for expected in [
            "cls_deco",
            "base_factory",
            "class_body_helper",
            "method_deco",
            "default_factory",
        ] {
            assert!(
                names.contains(&expected.to_string()),
                "{expected} should appear in class outgoing; got: {names:?}"
            );
        }
    }

    #[test]
    fn outgoing_function_excludes_nested_def_body() {
        // The outer function should NOT include calls from inside a nested
        // `def`'s body; the user navigates to `nested` and expands it
        // separately.
        let test = cursor_test(
            r#"
            def baz():
                pass

            def out<CURSOR>er():
                def nested():
                    baz()  # belongs to `nested`, not `outer`
                nested()
            "#,
        );
        let names: Vec<_> = test
            .outgoing()
            .into_iter()
            .map(|c| c.to.name.as_str().to_string())
            .collect();
        assert!(
            !names.contains(&"baz".to_string()),
            "nested def body call should NOT leak to outer; got: {names:?}"
        );
        assert!(
            names.contains(&"nested".to_string()),
            "outer should still see `nested` as a callee; got: {names:?}"
        );
    }

    #[test]
    fn outgoing_function_includes_param_default() {
        // Parameter defaults are evaluated at definition time and belong to
        // the function's own outgoing edges.
        let test = cursor_test(
            r#"
            def default_factory():
                return None

            def f<CURSOR>oo(x=default_factory()):
                pass
            "#,
        );
        let names: Vec<_> = test
            .outgoing()
            .into_iter()
            .map(|c| c.to.name.as_str().to_string())
            .collect();
        assert!(
            names.contains(&"default_factory".to_string()),
            "param default call should appear in outgoing; got: {names:?}"
        );
    }

    #[test]
    fn outgoing_class_with_base_call() {
        // Calls inside a class's base list are evaluated when the class
        // statement runs.
        let test = cursor_test(
            r#"
            def base_factory():
                return object

            class De<CURSOR>rived(base_factory()):
                pass
            "#,
        );
        let names: Vec<_> = test
            .outgoing()
            .into_iter()
            .map(|c| c.to.name.as_str().to_string())
            .collect();
        assert!(
            names.contains(&"base_factory".to_string()),
            "base-class call should appear in outgoing; got: {names:?}"
        );
    }

    #[test]
    fn outgoing_lambda_default_attributed_to_enclosing_scope() {
        // A lambda's parameter defaults are evaluated when the surrounding
        // scope reaches the lambda expression — they belong to the enclosing
        // scope's outgoing, not to the lambda. The lambda's own body call is
        // NOT included.
        let test = cursor_test(
            r#"
            def default_factory():
                return None

            def lambda_body_helper():
                return 1

            def out<CURSOR>er():
                f = lambda x=default_factory(): lambda_body_helper()
                return f
            "#,
        );
        let names: Vec<_> = test
            .outgoing()
            .into_iter()
            .map(|c| c.to.name.as_str().to_string())
            .collect();
        assert!(
            names.contains(&"default_factory".to_string()),
            "lambda default should be in enclosing scope outgoing; got: {names:?}"
        );
        assert!(
            !names.contains(&"lambda_body_helper".to_string()),
            "lambda body call should NOT leak to enclosing scope; got: {names:?}"
        );
    }

    #[test]
    fn outgoing_skips_unresolved_calls() {
        let test = cursor_test(
            r#"
            def f<CURSOR>oo():
                print("hi")  # builtins resolve via stubs, so this *does* appear
                undefined_name()  # this should be skipped silently
            "#,
        );
        // Just verify we don't panic and that undefined_name doesn't appear.
        let outgoing = test.outgoing();
        assert!(
            outgoing
                .iter()
                .all(|c| c.to.name.as_str() != "undefined_name"),
            "undefined_name should be skipped, got: {:?}",
            outgoing
                .iter()
                .map(|c| c.to.name.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn outgoing_super_method_call() {
        // `super().m()` in a subclass method should record the parent class's
        // method as an outgoing target.
        let test = cursor_test(
            r#"
            class Base:
                def m(self):
                    pass

            class Child(Base):
                def m<CURSOR>(self):
                    super().m()
            "#,
        );
        let outgoing = test.outgoing();
        let names: Vec<_> = outgoing.iter().map(|c| c.to.name.as_str()).collect();
        assert!(
            names.contains(&"m"),
            "expected Base.m as an outgoing target, got: {names:?}",
        );
    }

    #[test]
    fn outgoing_multi_file() {
        // Outgoing calls should resolve across files for imported callees.
        let test = CursorTest::builder()
            .source(
                "lib.py",
                "
def helper():
    pass
",
            )
            .source(
                "main.py",
                "
from lib import helper

def f<CURSOR>oo():
    helper()
",
            )
            .build();
        let outgoing = test.outgoing();
        let names: Vec<_> = outgoing.iter().map(|c| c.to.name.as_str()).collect();
        assert!(
            names.contains(&"helper"),
            "expected cross-file `helper` as outgoing target, got: {names:?}",
        );
    }
}
