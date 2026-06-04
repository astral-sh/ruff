use crate::call_hierarchy::{CalleeLeaf, module_detail};
use crate::goto::{Definitions, GotoTarget, find_goto_target};
use crate::references::has_any_external_visible_definitions;
use crate::{CallHierarchyItem, Db, SymbolKind};
use rayon::prelude::*;
use ruff_db::files::File;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::helpers::is_dunder;
use ruff_python_ast::name::Name;
use ruff_python_ast::token::Tokens;
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use rustc_hash::FxHashMap;
use ty_project::parallel::{ParallelIteratorExt, minimum_parallel_job_len};
use ty_python_core::scope::{NodeWithScopeKind, ScopeKind};
use ty_python_semantic::types::ide_support::static_member_type_for_attribute;
use ty_python_semantic::types::{PropertyAccessorRole, Type};
use ty_python_semantic::{
    HasDefinition as _, HasType as _, ImportAliasResolution, SemanticModel, contains_identifier,
};

/// Salsa snapshots coordinate clone and drop through shared state. For ordinary targets, most
/// files are rejected by the text prefilter, so process enough files per job to amortize that
/// coordination. Use a lower minimum than references because matching files do more semantic work,
/// and dunder targets cannot use the prefilter.
const MAX_MIN_FILES_PER_PARALLEL_JOB: usize = 16;

/// Find every place in the project that calls the symbol at `offset`, grouped
/// by enclosing function/method/class/module.
pub fn incoming_calls(db: &dyn Db, file: File, offset: TextSize) -> Vec<IncomingCall> {
    let module = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let Some(goto_target) = find_goto_target(&model, &module, offset) else {
        return Vec::new();
    };

    let Some(target_definitions) =
        goto_target.definitions(&model, ImportAliasResolution::ResolveAliases)
    else {
        return Vec::new();
    };
    let is_externally_visible = has_any_external_visible_definitions(db, &target_definitions);
    let Some(target_definitions) = target_definitions.goto_declaration(&model, &goto_target) else {
        return Vec::new();
    };
    let Some(target_text) = goto_target.to_string() else {
        return Vec::new();
    };

    let needle: &str = target_text.as_ref();

    // Determine which property accessor role the user queried, if any. Used
    // at attribute-reference call sites to discard co-definitions of the
    // wrong role (e.g. when querying a setter, don't let the getter
    // co-definition pull in every read site).
    let target_role = match &goto_target {
        GotoTarget::FunctionDef(function) => function
            .inferred_type(&model)
            .and_then(Type::as_property_instance)
            .and_then(|property| property.accessor_role(db, function.definition(&model))),
        _ => None,
    };

    // Attribute leaves for an ordinary target can only match the queried name.
    // Bare-name calls may route through aliases and are checked semantically.
    // Dunder methods may be invoked without spelling their name (`value()`
    // invokes `__call__`, and `C()` invokes `__init__`), so do not use a
    // text filter for them.
    let needle = (!is_dunder(needle)).then_some(needle);

    // Collect raw `(caller_file, call_site_range, enclosing_scope)` triples.
    let mut raw = call_sites_for_file(db, file, &target_definitions, target_role, needle);

    if is_externally_visible {
        let files = db.project().files(db);
        let files: Vec<_> = files
            .iter()
            .copied()
            .filter(|other| *other != file)
            .collect();
        let minimum_job_len = minimum_parallel_job_len(files.len(), MAX_MIN_FILES_PER_PARALLEL_JOB);
        // The byte-level text prefilter still pays off as a coarse gate:
        // files that don't contain the target name (or an import of it)
        // textually are skipped before any AST work. Files that route the
        // call through an alias (`from m import foo as bar; bar()`) still
        // pass the gate because they contain `foo` in the import line.
        // Dunder calls have no required textual spelling, so the filter
        // is disabled for them.
        let other_sites = files
            .into_par_iter()
            .with_min_len(minimum_job_len)
            .map_with_db(db, |db, other_file| {
                let source = ruff_db::source::source_text(db, other_file);
                if let Some(name) = needle
                    && !contains_identifier(&source, name)
                {
                    return Vec::new();
                }

                call_sites_for_file(db, other_file, &target_definitions, target_role, needle)
            })
            .flat_map_iter(|sites| sites)
            .collect::<Vec<_>>();

        raw.extend(other_sites);
    }

    // Group by (enclosing scope file, enclosing scope selection range).
    let mut groups: FxHashMap<EnclosingKey, (CallHierarchyItem, Vec<TextRange>)> =
        FxHashMap::default();
    for site in raw {
        let key = EnclosingKey {
            file: site.from.file,
            selection_range: site.from.selection_range,
        };
        groups
            .entry(key)
            .or_insert_with(|| (site.from, Vec::new()))
            .1
            .push(site.call_site_range);
    }

    let mut results: Vec<_> = groups
        .into_values()
        .map(|(from, mut from_ranges)| {
            from_ranges.sort_by_key(Ranged::start);
            from_ranges.dedup();
            IncomingCall { from, from_ranges }
        })
        .collect();

    results.sort_by(|a, b| {
        let a_path = a.from.file.path(db).as_str();
        let b_path = b.from.file.path(db).as_str();
        a_path.cmp(b_path).then_with(|| {
            a.from
                .selection_range
                .start()
                .cmp(&b.from.selection_range.start())
        })
    });
    results
}

#[derive(Debug, Clone)]
pub struct IncomingCall {
    /// The function/method/class/module that contains the call site(s).
    pub from: CallHierarchyItem,
    /// Call-site ranges inside `from.file`.
    pub from_ranges: Vec<TextRange>,
}

#[derive(PartialEq, Eq, Hash)]
struct EnclosingKey {
    file: File,
    selection_range: TextRange,
}

/// Walk one file's AST and record every call whose callee resolves to one of
/// `target_definitions`.
fn call_sites_for_file(
    db: &dyn Db,
    file: File,
    target_definitions: &Definitions<'_>,
    target_role: Option<PropertyAccessorRole>,
    needle: Option<&str>,
) -> Vec<RawCallSite> {
    let parsed = parsed_module(db, file);
    let module = parsed.load(db);
    let model = SemanticModel::new(db, file);
    let mut sites = Vec::new();

    let mut finder = CallSitesFinder {
        db,
        model: &model,
        module: &module,
        tokens: module.tokens(),
        target_definitions,
        target_role,
        needle,
        sites: &mut sites,
        ancestors: Vec::new(),
    };
    AnyNodeRef::from(module.syntax()).visit_source_order(&mut finder);

    sites
}

struct CallSitesFinder<'a, 'db> {
    db: &'db dyn Db,
    model: &'a SemanticModel<'db>,
    module: &'a ParsedModuleRef,
    tokens: &'a Tokens,
    target_definitions: &'a Definitions<'db>,
    /// Property accessor role the user originally queried (the definition the
    /// cursor was on), or `None` when the queried symbol is not a property
    /// accessor. Used at attribute sites to constrain which co-definitions in
    /// `target_definitions` are eligible matches. Without this, querying a
    /// setter would also match reads (via the getter co-definition).
    target_role: Option<PropertyAccessorRole>,
    /// Name an attribute leaf must spell before semantic resolution, or
    /// `None` for dunders that can be invoked without spelling their name.
    /// Bare-name leaves are not gated by this and always resolve semantically.
    needle: Option<&'a str>,
    sites: &'a mut Vec<RawCallSite>,
    ancestors: Vec<AnyNodeRef<'a>>,
}

impl<'a> SourceOrderVisitor<'a> for CallSitesFinder<'a, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        self.ancestors.push(node);

        match node {
            // Walk by call site rather than by identifier. This is structurally
            // faster (call sites are much rarer than identifier occurrences)
            // and — critically — it makes alias-routed calls work: `bar()`
            // where `bar` is a local rebinding/alias of the target resolves
            // semantically without needing the alias name in the text needle.
            AnyNodeRef::ExprCall(call) => {
                if let Some(leaf) = CalleeLeaf::from_expr(&call.func)
                    && self.leaf_matches_needle(leaf)
                {
                    self.check_call_site(leaf, AnyNodeRef::from(call));
                }
            }
            AnyNodeRef::Decorator(decorator) => {
                // `@foo` without parens is a runtime call; `@foo()` is handled
                // by the `ExprCall` arm above.
                if let Some(leaf) = CalleeLeaf::from_expr(&decorator.expression)
                    && self.leaf_matches_needle(leaf)
                {
                    self.check_call_site(leaf, AnyNodeRef::from(&decorator.expression));
                }
            }
            // A property access is an implicit invocation of its matching
            // accessor. Skip attributes used as explicit callees because the
            // `ExprCall` / `Decorator` arms already record those sites.
            AnyNodeRef::ExprAttribute(attribute) => {
                if !attribute_is_callee_of_parent(&self.ancestors, attribute)
                    && self.attribute_name_could_match(attribute.attr.as_str())
                {
                    self.check_property_access(attribute);
                }
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

impl<'a> CallSitesFinder<'a, '_> {
    /// Text-level prefilter for call-site leaves. Ordinary attribute leaves
    /// must spell the queried name; bare-name leaves always resolve
    /// semantically because they can route through aliases.
    fn leaf_matches_needle(&self, leaf: CalleeLeaf<'_>) -> bool {
        match leaf {
            CalleeLeaf::Name(_) => true,
            CalleeLeaf::AttrIdentifier { identifier, .. } => {
                self.attribute_name_could_match(identifier.as_str())
            }
        }
    }

    fn attribute_name_could_match(&self, name: &str) -> bool {
        self.needle.is_none_or(|target_name| target_name == name)
    }

    fn check_call_site(&mut self, leaf: CalleeLeaf<'a>, scope_node: AnyNodeRef<'a>) {
        let Some((goto_target, call_site_range)) =
            leaf.resolve(self.model, self.tokens, &self.ancestors)
        else {
            return;
        };

        // Keep callable implementations here rather than applying
        // `goto_declaration`: clicking the name in `C()` deliberately navigates
        // to `C`, but the expression is also an incoming call to `C.__init__`
        // or `C.__new__`.
        let Some(current_definitions) =
            goto_target.definitions(self.model, ImportAliasResolution::ResolveAliases)
        else {
            return;
        };
        if !self.target_definitions.intersects(&current_definitions) {
            return;
        }

        let from = self.enclosing_scope_item(scope_node);
        self.sites.push(RawCallSite {
            from,
            call_site_range,
        });
    }

    /// Record `@property` accesses as implicit invocations of their matching
    /// accessor: a read calls the getter, a write calls the setter, and a
    /// `del` calls the deleter.
    fn check_property_access(&mut self, attribute: &'a ast::ExprAttribute) {
        let Some(Type::PropertyInstance(property)) =
            static_member_type_for_attribute(self.model, attribute)
        else {
            return;
        };

        let leaf = CalleeLeaf::AttrIdentifier {
            attribute,
            identifier: &attribute.attr,
        };
        // Strip the trailing self-push so the slice mirrors what `ExprCall` and
        // `Decorator` pass to `resolve_callee` (they resolve before descending).
        let ancestors_without_self = &self.ancestors[..self.ancestors.len() - 1];
        let Some((goto_target, call_site_range)) =
            leaf.resolve(self.model, self.tokens, ancestors_without_self)
        else {
            return;
        };

        let Some(current_definitions) = goto_target
            .definitions(self.model, ImportAliasResolution::ResolveAliases)
            .and_then(|d| d.goto_declaration(self.model, &goto_target))
        else {
            return;
        };

        // Route the site by access kind. Without this filter, a read of
        // `c.prop` would also match the setter when both accessors are
        // co-definitions in `target_definitions`.
        let intersects = current_definitions.iter().any(|resolved| {
            let role = resolved
                .definition()
                .and_then(|def| property.accessor_role(self.db, def));
            let matches_site_kind = match attribute.ctx {
                ast::ExprContext::Load => {
                    matches!(role, Some(PropertyAccessorRole::Getter) | None)
                }
                ast::ExprContext::Store => matches!(role, Some(PropertyAccessorRole::Setter)),
                ast::ExprContext::Del => matches!(role, Some(PropertyAccessorRole::Deleter)),
                ast::ExprContext::Invalid => false,
            };
            if !matches_site_kind {
                return false;
            }

            // Setter and deleter definitions include the getter as a
            // co-definition. Only the queried accessor contributes sites.
            if matches!(
                self.target_role,
                Some(PropertyAccessorRole::Setter | PropertyAccessorRole::Deleter)
            ) && role != self.target_role
            {
                return false;
            }
            self.target_definitions
                .iter()
                .any(|target| target == resolved)
        });
        if !intersects {
            return;
        }

        let from = self.enclosing_scope_item(AnyNodeRef::from(attribute));
        self.sites.push(RawCallSite {
            from,
            call_site_range,
        });
    }

    /// Build the item for the semantic scope in which a call site is evaluated.
    ///
    /// This differs from taking the nearest syntactic function/class ancestor for
    /// expressions attached to definitions: a method decorator or parameter
    /// default is evaluated in its class scope, even though it is nested below the
    /// method's AST node. Comprehension and annotation scopes have no callable
    /// hierarchy item of their own, so walk outward until reaching one that does.
    fn enclosing_scope_item(&self, scope_node: AnyNodeRef<'_>) -> CallHierarchyItem {
        let file = self.model.file();
        let mut ancestors = self.model.ancestor_scopes(scope_node);
        let Some((_, enclosing)) = ancestors.find(|(_, ancestor)| {
            matches!(
                ancestor.kind(),
                ScopeKind::Module | ScopeKind::Function | ScopeKind::Class | ScopeKind::Lambda
            )
        }) else {
            return module_item(self.db, file);
        };

        match enclosing.node() {
            NodeWithScopeKind::Module => module_item(self.db, file),
            NodeWithScopeKind::Function(func) => {
                let func = func.node(self.module);
                let is_method = ancestors
                    .find_map(|(_, ancestor)| match ancestor.kind() {
                        ScopeKind::Class => Some(true),
                        ScopeKind::Module | ScopeKind::Function | ScopeKind::Lambda => Some(false),
                        ScopeKind::TypeParams | ScopeKind::Comprehension | ScopeKind::TypeAlias => {
                            None
                        }
                    })
                    .unwrap_or(false);
                CallHierarchyItem {
                    name: func.name.id.clone(),
                    kind: if is_method {
                        SymbolKind::Method
                    } else {
                        SymbolKind::Function
                    },
                    detail: module_detail(self.db, file),
                    file,
                    full_range: func.range(),
                    selection_range: func.name.range(),
                }
            }
            NodeWithScopeKind::Class(class) => {
                let class = class.node(self.module);
                CallHierarchyItem {
                    name: class.name.id.clone(),
                    kind: SymbolKind::Class,
                    detail: module_detail(self.db, file),
                    file,
                    full_range: class.range(),
                    selection_range: class.name.range(),
                }
            }
            NodeWithScopeKind::Lambda(lambda) => {
                let lambda = lambda.node(self.module);
                let end = lambda
                    .parameters
                    .as_deref()
                    .map(Ranged::end)
                    .unwrap_or(lambda.start() + "lambda".text_len());

                CallHierarchyItem {
                    name: Name::new_static("(lambda)"),
                    kind: SymbolKind::Function,
                    detail: module_detail(self.db, file),
                    file,
                    full_range: lambda.range(),
                    selection_range: TextRange::new(lambda.start(), end),
                }
            }
            _ => module_item(self.db, file),
        }
    }
}

struct RawCallSite {
    from: CallHierarchyItem,
    call_site_range: TextRange,
}

/// Build an item for the module-level enclosing scope (no enclosing function).
fn module_item(db: &dyn Db, file: File) -> CallHierarchyItem {
    let name = ty_module_resolver::file_to_module(db, file)
        .map(|module| Name::new(module.name(db).last_component()))
        .unwrap_or_else(|| Name::new_static("<module>"));
    CallHierarchyItem {
        name,
        kind: SymbolKind::Module,
        detail: None,
        file,
        full_range: TextRange::default(),
        selection_range: TextRange::default(),
    }
}

/// Returns `true` when `attribute` is the immediate callee of an enclosing
/// `ExprCall` or `Decorator`. The `ExprCall` / `Decorator` arms in
/// `CallSitesFinder::enter_node` already record those sites — skipping here
/// avoids the descriptor-+-call double-count pyright exhibits.
fn attribute_is_callee_of_parent<'a>(
    ancestors: &[AnyNodeRef<'a>],
    attribute: &'a ast::ExprAttribute,
) -> bool {
    // `enter_node` has already pushed `attribute` onto `ancestors`, so the
    // parent is at index `len - 2`.
    let Some(parent_idx) = ancestors.len().checked_sub(2) else {
        return false;
    };
    let attribute_range = attribute.range();
    match ancestors[parent_idx] {
        AnyNodeRef::ExprCall(call) => call.func.range() == attribute_range,
        AnyNodeRef::Decorator(decorator) => decorator.expression.range() == attribute_range,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        call_hierarchy::snapshot_item_label,
        outgoing_calls,
        tests::{CursorTest, IntoDiagnostic, cursor_test},
    };
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span, SubDiagnostic,
        SubDiagnosticSeverity,
    };

    use super::*;

    impl CursorTest {
        fn incoming_calls(&self) -> String {
            let Some(target) = self
                .prepare_calls()
                .and_then(|items| items.into_iter().next())
            else {
                return "No incoming calls found".to_string();
            };
            let calls = incoming_calls(&self.db, target.file, target.selection_range.start());
            if calls.is_empty() {
                return "No incoming calls found".to_string();
            }
            let target_name = target.name.to_string();

            self.render_diagnostics(calls.into_iter().map(|call| IncomingCallDiagnostic {
                target_name: target_name.clone(),
                call,
            }))
        }
    }

    struct IncomingCallDiagnostic {
        target_name: String,
        call: IncomingCall,
    }

    impl IntoDiagnostic for IncomingCallDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let IncomingCall { from, from_ranges } = self.call;
            let mut diagnostic = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("incoming-calls")),
                Severity::Info,
                format!("Incoming calls to `{}`", self.target_name),
            );

            for range in from_ranges {
                diagnostic.annotate(
                    Annotation::primary(Span::from(from.file).with_range(range))
                        .message("Call site"),
                );
            }

            let mut caller =
                SubDiagnostic::new(SubDiagnosticSeverity::Info, snapshot_item_label(&from));
            let mut caller_annotation =
                Annotation::primary(Span::from(from.file).with_range(from.selection_range));
            if matches!(&from.kind, SymbolKind::Module) {
                caller_annotation.hide_snippet(true);
            }
            caller.annotate(caller_annotation);
            diagnostic.sub(caller);

            diagnostic
        }
    }

    #[test]
    fn single_file() {
        let test = cursor_test(
            r#"
            def f<CURSOR>oo():
                pass

            def caller():
                foo()
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `foo`
         --> main.py:6:5
          |
        6 |     foo()
          |     ^^^ Call site
          |
        info: Function: `caller` (`main`)
         --> main.py:5:5
          |
        5 | def caller():
          |     ^^^^^^
          |
        ");
    }

    #[test]
    fn non_call_reference_filtered_out() {
        let test = cursor_test(
            r#"
            def f<CURSOR>oo():
                pass

            def caller():
                cb = foo  # not a call — should NOT appear
                foo()     # this is a call — should appear once
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `foo`
         --> main.py:7:5
          |
        7 |     foo()     # this is a call — should appear once
          |     ^^^ Call site
          |
        info: Function: `caller` (`main`)
         --> main.py:5:5
          |
        5 | def caller():
          |     ^^^^^^
          |
        ");
    }

    #[test]
    fn multi_file() {
        let test = CursorTest::builder()
            .source(
                "utils.py",
                "
def f<CURSOR>oo():
    pass
",
            )
            .source(
                "caller.py",
                "
from utils import foo

def use():
    foo()
",
            )
            .build();
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `foo`
         --> caller.py:5:5
          |
        5 |     foo()
          |     ^^^ Call site
          |
        info: Function: `use` (`caller`)
         --> caller.py:4:5
          |
        4 | def use():
          |     ^^^
          |
        ");
    }

    #[test]
    fn via_import_alias() {
        let test = CursorTest::builder()
            .source(
                "utils.py",
                "
def f<CURSOR>oo():
    pass
",
            )
            .source(
                "caller.py",
                "
from utils import foo as bar

def use():
    bar()
",
            )
            .build();
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `foo`
         --> caller.py:5:5
          |
        5 |     bar()
          |     ^^^ Call site
          |
        info: Function: `use` (`caller`)
         --> caller.py:4:5
          |
        4 | def use():
          |     ^^^
          |
        ");
    }

    #[test]
    fn multi_file_dunder_call_without_textual_method_name() {
        let test = CursorTest::builder()
            .source(
                "model.py",
                r#"
class Callable:
    def __ca<CURSOR>ll__(self) -> int:
        return 1
"#,
            )
            .source(
                "caller.py",
                r#"
from model import Callable

def invoke(value: Callable) -> int:
    return value()
"#,
            )
            .build();
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `__call__`
         --> caller.py:5:12
          |
        5 |     return value()
          |            ^^^^^ Call site
          |
        info: Function: `invoke` (`caller`)
         --> caller.py:4:5
          |
        4 | def invoke(value: Callable) -> int:
          |     ^^^^^^
          |
        ");
    }

    #[test]
    fn keyword_call() {
        let test = cursor_test(
            r#"
            def f<CURSOR>oo(x):
                pass

            def caller():
                foo(x=1)
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `foo`
         --> main.py:6:5
          |
        6 |     foo(x=1)
          |     ^^^ Call site
          |
        info: Function: `caller` (`main`)
         --> main.py:5:5
          |
        5 | def caller():
          |     ^^^^^^
          |
        ");
    }

    #[test]
    fn top_level_call_attributed_to_module() {
        let test = cursor_test(
            r#"
            def f<CURSOR>oo():
                pass

            foo()
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `foo`
         --> main.py:5:1
          |
        5 | foo()
          | ^^^ Call site
          |
        info: Module: `main`
        --> main.py:1:1
        ");
    }

    #[test]
    fn decorator_application() {
        // `@foo` (no parens) is a runtime call to `foo`.
        let test = cursor_test(
            r#"
            def f<CURSOR>oo(f):
                return f

            @foo
            def bar():
                pass
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `foo`
         --> main.py:5:2
          |
        5 | @foo
          |  ^^^ Call site
          |
        info: Module: `main`
        --> main.py:1:1
        ");
    }

    #[test]
    fn default_on_version_gated_method_attributed_to_class() {
        let test = CursorTest::builder()
            .python_version(ast::PythonVersion::PY311)
            .source(
                "main.py",
                r#"
import sys

def defa<CURSOR>ult() -> int:
    return 1

class C:
    if sys.version_info >= (3, 11):
        def method(self, value=default()):
            pass
"#,
            )
            .build();
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `default`
         --> main.py:9:32
          |
        9 |         def method(self, value=default()):
          |                                ^^^^^^^ Call site
          |
        info: Class: `C` (`main`)
         --> main.py:7:7
          |
        7 | class C:
          |       ^
          |
        ");
    }

    #[test]
    fn method_does_not_confuse_with_same_name_on_other_class() {
        let test = cursor_test(
            r#"
            class A:
                def foo<CURSOR>(self):
                    pass

            class B:
                def foo(self):
                    pass

            def use(a: A, b: B):
                a.foo()
                b.foo()
            "#,
        );
        // Should only record the `a.foo()` site, not `b.foo()`.
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `foo`
          --> main.py:11:7
           |
        11 |     a.foo()
           |       ^^^ Call site
           |
        info: Function: `use` (`main`)
          --> main.py:10:5
           |
        10 | def use(a: A, b: B):
           |     ^^^
           |
        ");
    }

    #[test]
    fn super_method_call() {
        // `super().m()` in a subclass method should record the subclass method
        // as a caller of the parent class's method.
        let test = cursor_test(
            r#"
            class Base:
                def m<CURSOR>(self):
                    pass

            class Child(Base):
                def m(self):
                    super().m()
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `m`
         --> main.py:8:17
          |
        8 |         super().m()
          |                 ^ Call site
          |
        info: Method: `m` (`main`)
         --> main.py:7:9
          |
        7 |     def m(self):
          |         ^
          |
        ");
    }

    #[test]
    fn multi_file_constructor_call_resolves_to_init_without_textual_method_name() {
        let test = CursorTest::builder()
            .source(
                "model.py",
                r#"
class C:
    def __in<CURSOR>it__(self) -> None:
        pass
"#,
            )
            .source(
                "caller.py",
                r#"
from model import C

def make() -> C:
    return C()
"#,
            )
            .build();
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `__init__`
         --> caller.py:5:12
          |
        5 |     return C()
          |            ^ Call site
          |
        info: Function: `make` (`caller`)
         --> caller.py:4:5
          |
        4 | def make() -> C:
          |     ^^^^
          |
        ");
    }

    // --- incoming: attribute-reference call sites --------------------------
    //
    // Beyond `ExprCall` and `Decorator`, a `@property` access is an implicit
    // invocation of the getter, setter, or deleter through the descriptor
    // protocol.

    #[test]
    fn property_getter_read() {
        let test = cursor_test(
            r#"
            class C:
                @property
                def pr<CURSOR>op(self) -> int:
                    return 1

            def read(c: C) -> int:
                return c.prop
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `prop`
         --> main.py:8:14
          |
        8 |     return c.prop
          |              ^^^^ Call site
          |
        info: Function: `read` (`main`)
         --> main.py:7:5
          |
        7 | def read(c: C) -> int:
          |     ^^^^
          |
        ");
    }

    #[test]
    fn property_setter_write() {
        // Querying the setter must surface the assignment (`c.prop = 5`) but
        // not the read (`c.prop`) — pyright lumps them; we don't.
        let test = cursor_test(
            r#"
            class C:
                @property
                def prop(self) -> int:
                    return self._v

                @prop.setter
                def pr<CURSOR>op(self, v: int) -> None:
                    self._v = v

            def write(c: C) -> None:
                c.prop = 5

            def read(c: C) -> int:
                return c.prop
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `prop`
          --> main.py:12:7
           |
        12 |     c.prop = 5
           |       ^^^^ Call site
           |
        info: Function: `write` (`main`)
          --> main.py:11:5
           |
        11 | def write(c: C) -> None:
           |     ^^^^^
           |
        ");
    }

    #[test]
    fn property_deleter_del() {
        let test = cursor_test(
            r#"
            class C:
                @property
                def prop(self) -> int:
                    return self._v

                @prop.deleter
                def pr<CURSOR>op(self) -> None:
                    del self._v

            def remove(c: C) -> None:
                del c.prop

            def read(c: C) -> int:
                return c.prop
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `prop`
          --> main.py:12:11
           |
        12 |     del c.prop
           |           ^^^^ Call site
           |
        info: Function: `remove` (`main`)
          --> main.py:11:5
           |
        11 | def remove(c: C) -> None:
           |     ^^^^^^
           |
        ");
    }

    #[test]
    fn bound_method_reference_passed_as_arg_is_not_a_call() {
        // Passing a bound-method value does not execute the method body.
        let test = cursor_test(
            r#"
            def make_async(fn, executor=None):
                return fn

            class C:
                def metho<CURSOR>d(self) -> int:
                    return 1

                def __init__(self) -> None:
                    self._async = make_async(self.method)
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"No incoming calls found");
    }

    #[test]
    fn bound_method_reference_assigned_is_not_a_call() {
        // Binding a method for later invocation is a reference, not a call.
        let test = cursor_test(
            r#"
            class C:
                def metho<CURSOR>d(self) -> int:
                    return 1

                def setup(self) -> None:
                    cb = self.method
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"No incoming calls found");
    }

    #[test]
    fn method_call() {
        let test = cursor_test(
            r#"
            class C:
                def metho<CURSOR>d(self) -> int:
                    return 1

            def use(c: C) -> int:
                return c.method()
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `method`
         --> main.py:7:14
          |
        7 |     return c.method()
          |              ^^^^^^ Call site
          |
        info: Function: `use` (`main`)
         --> main.py:6:5
          |
        6 | def use(c: C) -> int:
          |     ^^^
          |
        ");
    }

    #[test]
    fn non_callable_attribute_filtered() {
        // A plain instance attribute that isn't a function/method/property
        // must not show up in incomingCalls of anything.
        let test = cursor_test(
            r#"
            def func<CURSOR>():
                pass

            class C:
                def __init__(self) -> None:
                    self.func = 42  # local attribute, not the free function

            def use(c: C) -> None:
                _ = c.func
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"No incoming calls found");
    }

    #[test]
    fn lambda_caller_is_synthesized_item() {
        // A call inside a top-level lambda should be attributed to a
        // synthetic `(lambda)` item, selecting its callable header rather
        // than inventing an identifier range.
        let test = cursor_test(
            r#"
            def tar<CURSOR>get(x):
                pass

            f = lambda x: target(x)
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `target`
         --> main.py:5:15
          |
        5 | f = lambda x: target(x)
          |               ^^^^^^ Call site
          |
        info: Function: `(lambda)` (`main`)
         --> main.py:5:5
          |
        5 | f = lambda x: target(x)
          |     ^^^^^^^^
          |
        ");
        let Some(target) = test
            .prepare_calls()
            .and_then(|items| items.into_iter().next())
        else {
            panic!("expected a call hierarchy target");
        };
        let incoming = incoming_calls(&test.db, target.file, target.selection_range.start());
        // The selection identifies the anonymous callable header.
        let sel = incoming[0].from.selection_range;
        let source = test.cursor.source.as_str();
        assert_eq!(
            &source[sel.start().to_usize()..sel.end().to_usize()],
            "lambda x",
        );
    }

    #[test]
    fn two_lambdas_calling_same_function_two_distinct_items() {
        // Two separate lambdas, including one without parameters, must
        // surface as distinct `(lambda)` items with real selection ranges.
        let test = cursor_test(
            r#"
            def tar<CURSOR>get(x):
                pass

            a = lambda x: target(x)
            b = lambda: target(0)
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `target`
         --> main.py:5:15
          |
        5 | a = lambda x: target(x)
          |               ^^^^^^ Call site
          |
        info: Function: `(lambda)` (`main`)
         --> main.py:5:5
          |
        5 | a = lambda x: target(x)
          |     ^^^^^^^^
          |

        info[incoming-calls]: Incoming calls to `target`
         --> main.py:6:13
          |
        6 | b = lambda: target(0)
          |             ^^^^^^ Call site
          |
        info: Function: `(lambda)` (`main`)
         --> main.py:6:5
          |
        6 | b = lambda: target(0)
          |     ^^^^^^
          |
        ");
    }

    #[test]
    fn lambda_inside_function_attributed_to_lambda() {
        // A call inside a lambda nested in a function must be attributed
        // to the lambda, not to the enclosing function — the lambda is
        // the innermost callable scope.
        let test = cursor_test(
            r#"
            def tar<CURSOR>get(x):
                pass

            def outer_fn():
                f = lambda x: target(x)
                return f
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `target`
         --> main.py:6:19
          |
        6 |     f = lambda x: target(x)
          |                   ^^^^^^ Call site
          |
        info: Function: `(lambda)` (`main`)
         --> main.py:6:9
          |
        6 |     f = lambda x: target(x)
          |         ^^^^^^^^
          |
        ");
    }

    #[test]
    fn comprehension_attributed_to_enclosing_function() {
        // Comprehensions are NOT synthesized as items — a call inside a
        // list comprehension is still attributed to the enclosing named
        // scope (regression guard for "lambda only, not comprehensions").
        let test = cursor_test(
            r#"
            def tar<CURSOR>get(x):
                pass

            def caller(xs):
                return [target(x) for x in xs]
            "#,
        );
        assert_snapshot!(test.incoming_calls(), @"
        info[incoming-calls]: Incoming calls to `target`
         --> main.py:6:13
          |
        6 |     return [target(x) for x in xs]
          |             ^^^^^^ Call site
          |
        info: Function: `caller` (`main`)
         --> main.py:5:5
          |
        5 | def caller(xs):
          |     ^^^^^^
          |
        ");
    }

    #[test]
    fn lambda_follow_up_requests_are_leaves() {
        // Round-trip guard: the synthetic `(lambda)` item surfaced as a
        // caller has a `selection_range` whose start is the `lambda` keyword.
        // A follow-up `incomingCalls` / `outgoingCalls` request that lands
        // on that position has no `Definition` to resolve to, so both must
        // return empty — matching pyright's "lambda is a leaf" behavior.
        let test = cursor_test(
            r#"
            def tar<CURSOR>get(x):
                pass

            def helper(x):
                pass

            f = lambda x: target(helper(x))
            "#,
        );
        let Some(target) = test
            .prepare_calls()
            .and_then(|items| items.into_iter().next())
        else {
            panic!("expected a call hierarchy target");
        };
        let incoming = incoming_calls(&test.db, target.file, target.selection_range.start());
        assert_eq!(incoming.len(), 1, "got {incoming:?}");
        let lambda_item = &incoming[0].from;
        assert_eq!(lambda_item.name.as_str(), "(lambda)");

        let follow_up_incoming = incoming_calls(
            &test.db,
            lambda_item.file,
            lambda_item.selection_range.start(),
        );
        assert!(
            follow_up_incoming.is_empty(),
            "lambda must be a leaf for incomingCalls; got {follow_up_incoming:?}",
        );

        let follow_up_outgoing = outgoing_calls(
            &test.db,
            lambda_item.file,
            lambda_item.selection_range.start(),
        );
        assert!(
            follow_up_outgoing.is_empty(),
            "lambda must be a leaf for outgoingCalls; got {follow_up_outgoing:?}",
        );
    }
}
