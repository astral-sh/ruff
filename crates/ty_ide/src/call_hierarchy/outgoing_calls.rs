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
    use crate::tests::{CursorTest, IntoDiagnostic, cursor_test};
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span, SubDiagnostic,
        SubDiagnosticSeverity,
    };

    use super::*;

    impl CursorTest {
        fn outgoing_calls(&self) -> String {
            let Some(target) = self
                .prepare_calls()
                .and_then(|items| items.into_iter().next())
            else {
                return "No outgoing calls found".to_string();
            };
            let calls = outgoing_calls(&self.db, target.file, target.selection_range.start());
            if calls.is_empty() {
                return "No outgoing calls found".to_string();
            }
            let caller_name = target.name.to_string();

            self.render_diagnostics(calls.into_iter().map(|call| OutgoingCallDiagnostic {
                caller_name: caller_name.clone(),
                caller_file: target.file,
                call,
            }))
        }
    }

    struct OutgoingCallDiagnostic {
        caller_name: String,
        caller_file: File,
        call: OutgoingCall,
    }

    impl IntoDiagnostic for OutgoingCallDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let OutgoingCall { to, from_ranges } = self.call;
            let mut diagnostic = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("outgoing-calls")),
                Severity::Info,
                format!("Outgoing calls from `{}`", self.caller_name),
            );

            for range in from_ranges {
                diagnostic.annotate(
                    Annotation::primary(Span::from(self.caller_file).with_range(range))
                        .message("Call site"),
                );
            }

            let mut callee = SubDiagnostic::new(
                SubDiagnosticSeverity::Info,
                format!("Callee: `{}` ({})", to.name, to.kind.to_string()),
            );
            callee.annotate(Annotation::primary(
                Span::from(to.file).with_range(to.selection_range),
            ));
            diagnostic.sub(callee);

            diagnostic
        }
    }

    #[test]
    fn direct_call() {
        let test = cursor_test(
            r#"
            def helper():
                pass

            def f<CURSOR>oo():
                helper()
            "#,
        );
        assert_snapshot!(test.outgoing_calls(), @"
        info[outgoing-calls]: Outgoing calls from `foo`
         --> main.py:6:5
          |
        6 |     helper()
          |     ^^^^^^ Call site
          |
        info: Callee: `helper` (Function)
         --> main.py:2:5
          |
        2 | def helper():
          |     ^^^^^^
          |
        ");
    }

    #[test]
    fn attribute_call() {
        let test = cursor_test(
            r#"
            class C:
                def m(self):
                    pass

            def f<CURSOR>oo(c: C):
                c.m()
            "#,
        );
        assert_snapshot!(test.outgoing_calls(), @"
        info[outgoing-calls]: Outgoing calls from `foo`
         --> main.py:7:7
          |
        7 |     c.m()
          |       ^ Call site
          |
        info: Callee: `m` (Method)
         --> main.py:3:9
          |
        3 |     def m(self):
          |         ^
          |
        ");
    }

    #[test]
    fn constructor_call() {
        let test = cursor_test(
            r#"
            class C:
                pass

            def f<CURSOR>oo():
                C()
            "#,
        );
        assert_snapshot!(test.outgoing_calls(), @"
        info[outgoing-calls]: Outgoing calls from `foo`
         --> main.py:6:5
          |
        6 |     C()
          |     ^ Call site
          |
        info: Callee: `C` (Class)
         --> main.py:2:7
          |
        2 | class C:
          |       ^
          |
        ");
    }

    #[test]
    fn multiple_calls_to_same_callee() {
        let test = cursor_test(
            r#"
            def helper():
                pass

            def f<CURSOR>oo():
                helper()
                helper()
            "#,
        );
        assert_snapshot!(test.outgoing_calls(), @"
        info[outgoing-calls]: Outgoing calls from `foo`
         --> main.py:6:5
          |
        6 |     helper()
          |     ^^^^^^ Call site
        7 |     helper()
          |     ^^^^^^ Call site
          |
        info: Callee: `helper` (Function)
         --> main.py:2:5
          |
        2 | def helper():
          |     ^^^^^^
          |
        ");
    }

    #[test]
    fn class_excludes_method_bodies() {
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
        assert_snapshot!(test.outgoing_calls(), @"No outgoing calls found");
    }

    #[test]
    fn class_includes_decorators_bases_and_class_body() {
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
        assert_snapshot!(test.outgoing_calls(), @"
        info[outgoing-calls]: Outgoing calls from `Cls`
          --> main.py:17:2
           |
        17 | @cls_deco
           |  ^^^^^^^^ Call site
           |
        info: Callee: `cls_deco` (Function)
         --> main.py:2:5
          |
        2 | def cls_deco(cls):
          |     ^^^^^^^^
          |

        info[outgoing-calls]: Outgoing calls from `Cls`
          --> main.py:18:11
           |
        18 | class Cls(base_factory()):
           |           ^^^^^^^^^^^^ Call site
           |
        info: Callee: `base_factory` (Function)
         --> main.py:5:5
          |
        5 | def base_factory():
          |     ^^^^^^^^^^^^
          |

        info[outgoing-calls]: Outgoing calls from `Cls`
          --> main.py:19:12
           |
        19 |     attr = class_body_helper()
           |            ^^^^^^^^^^^^^^^^^ Call site
           |
        info: Callee: `class_body_helper` (Function)
         --> main.py:8:5
          |
        8 | def class_body_helper():
          |     ^^^^^^^^^^^^^^^^^
          |

        info[outgoing-calls]: Outgoing calls from `Cls`
          --> main.py:21:6
           |
        21 |     @method_deco
           |      ^^^^^^^^^^^ Call site
           |
        info: Callee: `method_deco` (Function)
          --> main.py:11:5
           |
        11 | def method_deco(fn):
           |     ^^^^^^^^^^^
           |

        info[outgoing-calls]: Outgoing calls from `Cls`
          --> main.py:22:19
           |
        22 |     def m(self, x=default_factory()):
           |                   ^^^^^^^^^^^^^^^ Call site
           |
        info: Callee: `default_factory` (Function)
          --> main.py:14:5
           |
        14 | def default_factory():
           |     ^^^^^^^^^^^^^^^
           |
        ");
    }

    #[test]
    fn function_excludes_nested_def_body() {
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
        assert_snapshot!(test.outgoing_calls(), @"
        info[outgoing-calls]: Outgoing calls from `outer`
         --> main.py:8:5
          |
        8 |     nested()
          |     ^^^^^^ Call site
          |
        info: Callee: `nested` (Function)
         --> main.py:6:9
          |
        6 |     def nested():
          |         ^^^^^^
          |
        ");
    }

    #[test]
    fn function_includes_param_default() {
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
        assert_snapshot!(test.outgoing_calls(), @"
        info[outgoing-calls]: Outgoing calls from `foo`
         --> main.py:5:11
          |
        5 | def foo(x=default_factory()):
          |           ^^^^^^^^^^^^^^^ Call site
          |
        info: Callee: `default_factory` (Function)
         --> main.py:2:5
          |
        2 | def default_factory():
          |     ^^^^^^^^^^^^^^^
          |
        ");
    }

    #[test]
    fn class_with_base_call() {
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
        assert_snapshot!(test.outgoing_calls(), @"
        info[outgoing-calls]: Outgoing calls from `Derived`
         --> main.py:5:15
          |
        5 | class Derived(base_factory()):
          |               ^^^^^^^^^^^^ Call site
          |
        info: Callee: `base_factory` (Function)
         --> main.py:2:5
          |
        2 | def base_factory():
          |     ^^^^^^^^^^^^
          |
        ");
    }

    #[test]
    fn lambda_default_attributed_to_enclosing_scope() {
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
        assert_snapshot!(test.outgoing_calls(), @"
        info[outgoing-calls]: Outgoing calls from `outer`
         --> main.py:9:18
          |
        9 |     f = lambda x=default_factory(): lambda_body_helper()
          |                  ^^^^^^^^^^^^^^^ Call site
          |
        info: Callee: `default_factory` (Function)
         --> main.py:2:5
          |
        2 | def default_factory():
          |     ^^^^^^^^^^^^^^^
          |
        ");
    }

    #[test]
    fn skips_unresolved_calls() {
        let test = cursor_test(
            r#"
            def f<CURSOR>oo():
                print("hi")  # builtins resolve via stubs, so this *does* appear
                undefined_name()  # this should be skipped silently
            "#,
        );
        assert_snapshot!(test.outgoing_calls(), @r#"
        info[outgoing-calls]: Outgoing calls from `foo`
         --> main.py:3:5
          |
        3 |     print("hi")  # builtins resolve via stubs, so this *does* appear
          |     ^^^^^ Call site
          |
        info: Callee: `print` (Function)
            --> stdlib/builtins.pyi:4367:5
             |
        4367 | def print(
             |     ^^^^^
             |

        info[outgoing-calls]: Outgoing calls from `foo`
         --> main.py:3:5
          |
        3 |     print("hi")  # builtins resolve via stubs, so this *does* appear
          |     ^^^^^ Call site
          |
        info: Callee: `print` (Function)
            --> stdlib/builtins.pyi:4386:5
             |
        4386 | def print(
             |     ^^^^^
             |
        "#);
    }

    #[test]
    fn super_method_call() {
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
        assert_snapshot!(test.outgoing_calls(), @"
        info[outgoing-calls]: Outgoing calls from `m`
         --> main.py:8:17
          |
        8 |         super().m()
          |                 ^ Call site
          |
        info: Callee: `m` (Method)
         --> main.py:3:9
          |
        3 |     def m(self):
          |         ^
          |

        info[outgoing-calls]: Outgoing calls from `m`
         --> main.py:8:9
          |
        8 |         super().m()
          |         ^^^^^ Call site
          |
        info: Callee: `super` (Class)
           --> stdlib/builtins.pyi:316:7
            |
        316 | class super:
            |       ^^^^^
            |
        ");
    }

    #[test]
    fn multi_file() {
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
        assert_snapshot!(test.outgoing_calls(), @"
        info[outgoing-calls]: Outgoing calls from `foo`
         --> main.py:5:5
          |
        5 |     helper()
          |     ^^^^^^ Call site
          |
        info: Callee: `helper` (Function)
         --> lib.py:2:5
          |
        2 | def helper():
          |     ^^^^^^
          |
        ");
    }
}
