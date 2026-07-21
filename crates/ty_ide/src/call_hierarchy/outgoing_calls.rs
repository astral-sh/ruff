use std::collections::hash_map::Entry;

use crate::call_hierarchy::CalleeLeaf;
use crate::goto::find_goto_target;
use crate::{CallHierarchyItem, Db};
use ruff_db::PythonFile;
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

/// Find the callees associated with the function, method, or class at `offset`.
///
/// Calls in the item's body are reported as outgoing calls of that item. Calls
/// in a declaration attached to the item, such as a decorator, annotation,
/// parameter default, type-parameter bound or default, or base-class
/// expression, are also reported for that item.
///
/// Nested function, class, and lambda bodies are not traversed; those calls
/// are reported when the nested callable is expanded separately. Declaration
/// expressions attached to a nested callable are still included while
/// traversing the containing item's body.
pub fn outgoing_calls(db: &dyn Db, file: PythonFile<'_>, offset: TextSize) -> Vec<OutgoingCall> {
    let module = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file.file(db));
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
    // one outgoing entry.
    let mut groups: FxHashMap<CalleeKey, (CallHierarchyItem, Vec<TextRange>)> =
        FxHashMap::default();

    for resolved in &definitions {
        let Some(def) = resolved.definition() else {
            continue;
        };
        let def_file = def.file(db);
        let def_parse_file = PythonFile::new(db, def_file, db.python_version());
        let parsed = parsed_module(db, def_parse_file).load(db);

        let model = SemanticModel::new(db, def_file);
        let mut finder = OutgoingCallsFinder {
            db,
            model: &model,
            tokens: parsed.tokens(),
            by_callee: &mut groups,
            ancestors: Vec::new(),
        };

        // Walk the queried symbol's signature parts (everything evaluated when
        // its `def`/`class` statement runs) and then its body. Inside the body
        // the visitor stops at nested callables — see `OutgoingCallsFinder`.
        match def.kind(db) {
            DefinitionKind::Function(fn_ref) => {
                let func = fn_ref.node(&parsed);
                finder.walk_callable_signature(
                    &func.decorator_list,
                    func.type_params.as_deref(),
                    Some(&func.parameters),
                    func.returns.as_deref(),
                );
                walk_body(&mut finder, &func.body);
            }
            DefinitionKind::Class(class_ref) => {
                let class = class_ref.node(&parsed);
                finder.walk_class_signature(
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
    /// Calls grouped by the callee. The value is the callee item alongside with all calls to it.
    by_callee: &'a mut FxHashMap<CalleeKey, (CallHierarchyItem, Vec<TextRange>)>,
    ancestors: Vec<AnyNodeRef<'a>>,
}

impl<'a> OutgoingCallsFinder<'a, '_> {
    fn record_callee(&mut self, leaf: CalleeLeaf<'a>) {
        let Some((goto_target, call_site_range)) =
            leaf.resolve(self.model, self.tokens, &self.ancestors)
        else {
            return;
        };

        let Some(definitions) = goto_target
            .definitions(self.model, ImportAliasResolution::ResolveAliases)
            .and_then(|d| d.goto_declaration(self.model, &goto_target))
        else {
            return;
        };

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
            let module_ref = parsed_module(
                self.db,
                PythonFile::new(self.db, def_file, self.db.python_version()),
            )
            .load(self.db);
            let selection_range = def.focus_range(self.db, &module_ref).range();

            let key = CalleeKey {
                file: def_file,
                selection_range,
            };

            match self.by_callee.entry(key) {
                Entry::Occupied(mut occupied) => {
                    occupied.get_mut().1.push(call_site_range);
                }
                Entry::Vacant(entry) => {
                    if let Some(item) =
                        CallHierarchyItem::from_definition(self.db, resolved, &module_ref)
                    {
                        entry.insert((item, vec![call_site_range]));
                    }
                }
            }
        }
    }

    /// Visit the definition-time parts of a function / lambda — everything
    /// evaluated when the `def` (or `lambda` expression) is reached at runtime
    /// in the *enclosing* scope: decorators, type parameters, parameter defaults
    /// and annotations, return-type annotation.
    fn walk_callable_signature(
        &mut self,
        decorator_list: &'a [ast::Decorator],
        type_params: Option<&'a ast::TypeParams>,
        parameters: Option<&'a ast::Parameters>,
        returns: Option<&'a ast::Expr>,
    ) {
        for decorator in decorator_list {
            walk_decorator(self, decorator);
        }
        if let Some(type_params) = type_params {
            walk_type_params(self, type_params);
        }
        if let Some(parameters) = parameters {
            walk_parameters(self, parameters);
        }
        if let Some(returns) = returns {
            walk_expr(self, returns);
        }
    }

    /// Visit the definition-time parts of a class statement: decorators, type
    /// parameters, base classes / keyword arguments / metaclass. The body is
    /// handled separately so the caller can decide whether to walk it.
    fn walk_class_signature(
        &mut self,
        decorator_list: &'a [ast::Decorator],
        type_params: Option<&'a ast::TypeParams>,
        arguments: Option<&'a ast::Arguments>,
    ) {
        for decorator in decorator_list {
            walk_decorator(self, decorator);
        }
        if let Some(type_params) = type_params {
            walk_type_params(self, type_params);
        }
        if let Some(arguments) = arguments {
            walk_arguments(self, arguments);
        }
    }
}

impl<'a> SourceOrderVisitor<'a> for OutgoingCallsFinder<'a, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        self.ancestors.push(node);

        match node {
            AnyNodeRef::ExprCall(call) => {
                if let Some(leaf) = CalleeLeaf::from_expr(&call.func) {
                    self.record_callee(leaf);
                }
            }
            AnyNodeRef::Decorator(decorator) => {
                // A bare `@foo` decorator with no parens is a runtime call. If
                // the user wrote `@foo()` we'll pick it up via the `ExprCall`
                // arm above instead.
                if let Some(leaf) = CalleeLeaf::from_expr(&decorator.expression) {
                    self.record_callee(leaf);
                }
            }
            // Nested callables: walk only the parts evaluated when the
            // surrounding scope runs (decorators, defaults, bases, ...). The
            // body belongs to the nested item itself and is reached by
            // expanding that item separately in the call hierarchy tree.
            AnyNodeRef::StmtFunctionDef(func) => {
                self.walk_callable_signature(
                    &func.decorator_list,
                    func.type_params.as_deref(),
                    Some(&func.parameters),
                    func.returns.as_deref(),
                );
                return TraversalSignal::Skip;
            }
            AnyNodeRef::StmtClassDef(class) => {
                self.walk_class_signature(
                    &class.decorator_list,
                    class.type_params.as_deref(),
                    class.arguments.as_deref(),
                );
                return TraversalSignal::Skip;
            }
            AnyNodeRef::ExprLambda(lambda) => {
                self.walk_callable_signature(&[], None, lambda.parameters.as_deref(), None);
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

#[cfg(test)]
mod tests {
    use crate::{
        call_hierarchy::snapshot_item_label,
        tests::{CursorTest, IntoDiagnostic, cursor_test},
    };
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
            let calls = outgoing_calls(
                &self.db,
                self.python_file(target.file),
                target.selection_range.start(),
            );
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

            let mut callee =
                SubDiagnostic::new(SubDiagnosticSeverity::Info, snapshot_item_label(&to));
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
        info: Function: `helper` (`main`)
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
        info: Method: `m` (`main`)
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
        info: Class: `C` (`main`)
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
        info: Function: `helper` (`main`)
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
        info: Function: `cls_deco` (`main`)
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
        info: Function: `base_factory` (`main`)
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
        info: Function: `class_body_helper` (`main`)
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
        info: Function: `method_deco` (`main`)
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
        info: Function: `default_factory` (`main`)
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
        info: Function: `nested` (`main`)
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
        info: Function: `default_factory` (`main`)
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
        info: Function: `base_factory` (`main`)
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
        info: Function: `default_factory` (`main`)
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
          --> main.py:LL:5
           |
        LL |     print("hi")  # builtins resolve via stubs, so this *does* appear
           |     ^^^^^ Call site
           |
        info: Function: `print` (`builtins`)
          --> stdlib/builtins.pyi:LL:5
           |
        LL | def print(
           |     ^^^^^
           |

        info[outgoing-calls]: Outgoing calls from `foo`
          --> main.py:LL:5
           |
        LL |     print("hi")  # builtins resolve via stubs, so this *does* appear
           |     ^^^^^ Call site
           |
        info: Function: `print` (`builtins`)
          --> stdlib/builtins.pyi:LL:5
           |
        LL | def print(
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
        info: Method: `m` (`main`)
         --> main.py:3:9
          |
        3 |     def m(self):
          |         ^
          |

        info[outgoing-calls]: Outgoing calls from `m`
          --> main.py:LL:9
           |
        LL |         super().m()
           |         ^^^^^ Call site
           |
        info: Class: `super` (`builtins`)
          --> stdlib/builtins.pyi:LL:7
           |
        LL | class super:
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
        info: Function: `helper` (`lib`)
         --> lib.py:2:5
          |
        2 | def helper():
          |     ^^^^^^
          |
        ");
    }
}
