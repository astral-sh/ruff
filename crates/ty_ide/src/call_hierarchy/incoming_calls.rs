use crate::call_hierarchy::{CalleeLeaf, RawCallSite, callee_leaf, resolve_callee};
use crate::goto::{Definitions, GotoTarget, find_goto_target};
use crate::references::{contains_identifier, has_any_external_visible_definitions};
use crate::{CallHierarchyItem, CallHierarchyItemKind, Db};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::helpers::is_dunder;
use ruff_python_ast::name::Name;
use ruff_python_ast::token::Tokens;
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange, TextSize};
use rustc_hash::FxHashMap;
use ty_python_semantic::types::ide_support::static_member_type_for_attribute;
use ty_python_semantic::types::{PropertyAccessorRole, Type};
use ty_python_semantic::{HasDefinition as _, HasType as _, ImportAliasResolution, SemanticModel};

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

    // Pre-compute the set of attribute names that *could* resolve to one of
    // `target_definitions`: the needle itself plus every distinct name carried
    // by the resolved definitions. Used as a cheap text-level prefilter at
    // attribute-leaf call sites (`obj.X` / `obj.X()` / `@obj.X`) — attribute
    // names are invariant under import aliasing, so a leaf whose `attr` is
    // outside this set cannot possibly resolve to the target. Bare-name leaves
    // (`X()`) can still route through aliases / rebindings and are deliberately
    // excluded from this filter so the existing alias support is preserved
    // (see `incoming_via_import_alias` and the comment on `CallSitesFinder`).
    //
    // The filter is disabled (left empty) when any candidate name is a
    // dunder method, because dunders are implicitly invoked through arbitrary
    // attribute syntax: `obj.fbank(...)` triggers `fbank.__call__`, and
    // `mod.MyClass(...)` triggers `MyClass.__init__`. In both cases the
    // textual leaf is the receiver name, not the dunder.
    let mut candidate_attribute_names: Vec<String> = Vec::new();
    candidate_attribute_names.push(needle.to_string());
    for resolved in &target_definitions {
        if let Some(def) = resolved.definition()
            && let Some(name) = def.name(db)
            && !candidate_attribute_names.iter().any(|n| n == &name)
        {
            candidate_attribute_names.push(name);
        }
    }
    if candidate_attribute_names.iter().any(|name| is_dunder(name)) {
        candidate_attribute_names.clear();
    }

    // Collect raw `(caller_file, call_site_range, enclosing_scope)` triples.
    let mut raw = call_sites_for_file(
        db,
        file,
        &target_definitions,
        target_role,
        &candidate_attribute_names,
    );

    if is_externally_visible {
        let result = std::sync::Mutex::new(Vec::<RawCallSite>::new());
        let files = db.project().files(db);
        {
            let db_clone = Db::dyn_clone(db);
            let target_definitions = &target_definitions;
            let files = &files;
            let result = &result;
            let candidate_attribute_names = &candidate_attribute_names;
            // The byte-level text prefilter still pays off as a coarse gate:
            // files that don't contain the target name (or an import of it)
            // textually are skipped before any AST work. Files that route the
            // call through an alias (`from m import foo as bar; bar()`) still
            // pass the gate because they contain `foo` in the import line, and
            // the in-file walk now resolves aliases semantically.
            rayon::scope(move |s| {
                for other_file in files {
                    if other_file == file {
                        continue;
                    }
                    let db = Db::dyn_clone(&*db_clone);
                    s.spawn(move |_| {
                        let db = &*db;
                        let source = ruff_db::source::source_text(db, other_file);
                        if !contains_identifier(&source, needle) {
                            return;
                        }
                        let sites = call_sites_for_file(
                            db,
                            other_file,
                            target_definitions,
                            target_role,
                            candidate_attribute_names,
                        );
                        result.lock().unwrap().extend(sites);
                    });
                }
            });
        }
        raw.extend(result.into_inner().unwrap());
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
    candidate_attribute_names: &[String],
) -> Vec<RawCallSite> {
    let parsed = parsed_module(db, file);
    let module = parsed.load(db);
    let model = SemanticModel::new(db, file);
    let mut sites = Vec::new();

    let mut finder = CallSitesFinder {
        db,
        model: &model,
        tokens: module.tokens(),
        target_definitions,
        target_role,
        candidate_attribute_names,
        sites: &mut sites,
        ancestors: Vec::new(),
    };
    AnyNodeRef::from(module.syntax()).visit_source_order(&mut finder);

    sites
}

struct CallSitesFinder<'a, 'db> {
    db: &'db dyn Db,
    model: &'a SemanticModel<'db>,
    tokens: &'a Tokens,
    target_definitions: &'a Definitions<'db>,
    /// Property accessor role the user originally queried (the definition the
    /// cursor was on), or `None` when the queried symbol is not a property
    /// accessor. Used at attribute sites to constrain which co-definitions in
    /// `target_definitions` are eligible matches. Without this, querying a
    /// setter would also match reads (via the getter co-definition).
    target_role: Option<PropertyAccessorRole>,
    /// Names that an attribute leaf could textually match before any
    /// semantic resolution. `obj.X` cannot resolve to a definition with a
    /// different name (attribute names are invariant under import aliasing),
    /// so leaves whose identifier is outside this set are skipped without a
    /// semantic query. Bare-name leaves are deliberately *not* gated by this
    /// (they may route through aliases / rebindings) and always go through
    /// the semantic check.
    candidate_attribute_names: &'a [String],
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
                if let Some(leaf) = callee_leaf(&call.func)
                    && self.leaf_could_match(leaf)
                {
                    self.check_call_site(leaf);
                }
            }
            AnyNodeRef::Decorator(decorator) => {
                // `@foo` without parens is a runtime call; `@foo()` is handled
                // by the `ExprCall` arm above.
                if let Some(leaf) = callee_leaf(&decorator.expression)
                    && self.leaf_could_match(leaf)
                {
                    self.check_call_site(leaf);
                }
            }
            // `obj.attr` references that aren't already the callee of an
            // enclosing `ExprCall` / `Decorator` (those arms recorded them).
            // Examples this catches:
            //   - `self.prop` where `prop` is a `@property` (descriptor invokes
            //     the getter body),
            //   - `make_async(self.method, ...)` where `self.method` is a
            //     bound-method reference passed as a callable,
            //   - `cb = self.method` assignment of a bound method.
            //
            // Bare `ExprName` references (e.g. `cb = foo` for a free function)
            // are deliberately *not* added here — pyright doesn't count them
            // either, and the `incoming_non_call_reference_filtered_out`
            // test depends on the bare-name filter staying in place.
            AnyNodeRef::ExprAttribute(attribute) => {
                if !attribute_is_callee_of_parent(&self.ancestors, attribute)
                    && self.attribute_name_could_match(attribute.attr.as_str())
                {
                    self.check_attribute_reference(attribute);
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
    /// Text-level prefilter for call-site leaves. Attribute leaves whose name
    /// is outside `candidate_attribute_names` cannot resolve to the target;
    /// bare-name leaves always go through the semantic check because they can
    /// route through aliases.
    fn leaf_could_match(&self, leaf: CalleeLeaf<'_>) -> bool {
        match leaf {
            CalleeLeaf::Name(_) => true,
            CalleeLeaf::AttrIdentifier { identifier, .. } => {
                self.attribute_name_could_match(identifier.as_str())
            }
        }
    }

    fn attribute_name_could_match(&self, name: &str) -> bool {
        // An empty candidate set means the prefilter is disabled (the target
        // includes a dunder method, which can be implicitly invoked through
        // any receiver name).
        self.candidate_attribute_names.is_empty()
            || self.candidate_attribute_names.iter().any(|n| n == name)
    }

    fn check_call_site(&mut self, leaf: CalleeLeaf<'a>) {
        let Some((goto_target, call_site_range)) =
            resolve_callee(self.model, self.tokens, &self.ancestors, leaf)
        else {
            return;
        };

        let Some(current_definitions) = goto_target
            .definitions(self.model, ImportAliasResolution::ResolveAliases)
            .and_then(|d| d.goto_declaration(self.model, &goto_target))
        else {
            return;
        };
        if !self.target_definitions.intersects(&current_definitions) {
            return;
        }

        let from = enclosing_scope_item(self.db, self.model.file(), &self.ancestors);
        self.sites.push(RawCallSite {
            from,
            call_site_range,
        });
    }

    /// Handle `obj.attr` reads/writes/dels that aren't already the callee of
    /// an enclosing `ExprCall` or `Decorator`. Treats `@property` access as an
    /// implicit invocation of the matching accessor (read → getter,
    /// write → setter, `del` → deleter), and treats unparenthesised bound-method
    /// references like `make_async(self.m, ...)` as call sites of `m`.
    fn check_attribute_reference(&mut self, attribute: &'a ast::ExprAttribute) {
        let leaf = CalleeLeaf::AttrIdentifier {
            attribute,
            identifier: &attribute.attr,
        };
        // Strip the trailing self-push so the slice mirrors what `ExprCall` and
        // `Decorator` pass to `resolve_callee` (they resolve before descending).
        let ancestors_without_self = &self.ancestors[..self.ancestors.len() - 1];
        let Some((goto_target, call_site_range)) =
            resolve_callee(self.model, self.tokens, ancestors_without_self, leaf)
        else {
            return;
        };

        let Some(current_definitions) = goto_target
            .definitions(self.model, ImportAliasResolution::ResolveAliases)
            .and_then(|d| d.goto_declaration(self.model, &goto_target))
        else {
            return;
        };

        // If the descriptor protocol is in play (the attribute resolves
        // statically to a `property` instance), route the site by access kind:
        // a read points at the getter, a write points at the setter, a `del`
        // points at the deleter. Without this filter, a read of `c.prop` would
        // also match the setter (when both accessors are co-definitions in
        // `target_definitions`) and pollute incoming-calls of the setter with
        // every read site. Non-property attributes (regular methods, attribute
        // reads of class names, …) pass through unchanged.
        let property = match static_member_type_for_attribute(self.model, attribute) {
            Some(Type::PropertyInstance(property)) => Some(property),
            _ => None,
        };
        if let Some(property) = property {
            let intersects = current_definitions.iter().any(|resolved| {
                let role = resolved
                    .definition()
                    .and_then(|def| property.accessor_role(self.db, def));
                // (1) Site-context filter: a read points at the getter, a
                //     write at the setter, a `del` at the deleter. Regular
                //     methods (role `None`) appear only on reads — writes
                //     and deletes of a non-property attribute aren't calls.
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
                // (2) Queried-role filter: discard co-definitions of the
                //     wrong role. When the user queried a setter,
                //     `target_definitions` also includes the getter (added by
                //     `property_getter_definitions`); without this filter,
                //     read sites would match through the getter co-def and
                //     pollute the setter's caller list.
                let matches_queried_role = match self.target_role {
                    None | Some(PropertyAccessorRole::Getter) => true,
                    Some(PropertyAccessorRole::Setter) => {
                        role == Some(PropertyAccessorRole::Setter)
                    }
                    Some(PropertyAccessorRole::Deleter) => {
                        role == Some(PropertyAccessorRole::Deleter)
                    }
                };
                if !matches_queried_role {
                    return false;
                }
                self.target_definitions.iter().any(|t| t == resolved)
            });
            if !intersects {
                return;
            }
        } else if !self.target_definitions.intersects(&current_definitions) {
            return;
        }

        let from = enclosing_scope_item(self.db, self.model.file(), &self.ancestors);
        self.sites.push(RawCallSite {
            from,
            call_site_range,
        });
    }
}

/// Build the enclosing-scope item by walking the ancestor stack outwards from
/// a call site until we find a `StmtFunctionDef` / `StmtClassDef` / `ExprLambda`;
/// if none is found, the enclosing scope is the module itself. Comprehensions
/// are deliberately skipped — they have no addressable identifier. Lambdas are
/// synthesized as `(lambda)` items (matching pyright) so the tree view can
/// attribute calls inside them to their source location instead of collapsing
/// them onto the enclosing named scope.
fn enclosing_scope_item(
    db: &dyn Db,
    file: File,
    ancestors: &[AnyNodeRef<'_>],
) -> CallHierarchyItem {
    // Find the innermost function/class/lambda ancestor.
    let mut iter = ancestors.iter().rev().enumerate();
    let innermost = iter.by_ref().find_map(|(idx, node)| match node {
        AnyNodeRef::StmtFunctionDef(func) => Some((idx, EnclosingNode::Function(func))),
        AnyNodeRef::StmtClassDef(class) => Some((idx, EnclosingNode::Class(class))),
        AnyNodeRef::ExprLambda(lambda) => Some((idx, EnclosingNode::Lambda(lambda))),
        _ => None,
    });
    let Some((_, innermost_node)) = innermost else {
        return module_item(db, file);
    };

    // Reuse the iterator (already advanced past `innermost`) to find what's
    // outside it. For a function, the nearest outer function-or-class tells us
    // method vs. nested function.
    let outer = iter.find_map(|(_, node)| match node {
        AnyNodeRef::StmtFunctionDef(_) => Some(false), // nested in another function
        AnyNodeRef::StmtClassDef(_) => Some(true),     // method on a class
        _ => None,
    });

    match innermost_node {
        EnclosingNode::Function(func) => CallHierarchyItem {
            name: Name::from(func.name.as_str().to_string()),
            kind: if outer.unwrap_or(false) {
                CallHierarchyItemKind::Method
            } else {
                CallHierarchyItemKind::Function
            },

            file,
            full_range: func.range(),
            selection_range: func.name.range(),
        },
        EnclosingNode::Class(class) => CallHierarchyItem {
            name: Name::from(class.name.as_str().to_string()),
            kind: CallHierarchyItemKind::Class,
            file,
            full_range: class.range(),
            selection_range: class.name.range(),
        },
        EnclosingNode::Lambda(lambda) => {
            let start = lambda.range().start();
            CallHierarchyItem {
                name: Name::from("(lambda)"),
                kind: CallHierarchyItemKind::Function,
                file,
                full_range: lambda.range(),
                selection_range: TextRange::at(start, TextSize::of("lambda")),
            }
        }
    }
}

enum EnclosingNode<'a> {
    Function(&'a ast::StmtFunctionDef),
    Class(&'a ast::StmtClassDef),
    Lambda(&'a ast::ExprLambda),
}

/// Build an item for the module-level enclosing scope (no enclosing function).
fn module_item(db: &dyn Db, file: File) -> CallHierarchyItem {
    let name = ty_module_resolver::file_to_module(db, file)
        .and_then(|m| {
            m.name(db)
                .to_string()
                .rsplit('.')
                .next()
                .map(str::to_string)
        })
        .unwrap_or_else(|| "<module>".to_string());
    CallHierarchyItem {
        name: Name::from(name),
        kind: CallHierarchyItemKind::Module,
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
        call_hierarchy::tests::snapshot_item,
        outgoing_calls,
        tests::{CursorTest, cursor_test},
    };

    use super::*;

    impl CursorTest {
        fn incoming(&self) -> Vec<IncomingCall> {
            let Some(items) = self.prepare_calls() else {
                return vec![];
            };
            let item = &items[0];
            incoming_calls(&self.db, item.file, item.selection_range.start())
        }
    }

    fn snapshot_incoming(db: &dyn Db, calls: &[IncomingCall]) -> String {
        calls
            .iter()
            .map(|call| {
                let head = snapshot_item(db, &call.from);
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

    #[test]
    fn incoming_single_file() {
        let test = cursor_test(
            r#"
            def f<CURSOR>oo():
                pass

            def caller():
                foo()
            "#,
        );
        insta::assert_snapshot!(snapshot_incoming(&test.db, &test.incoming()), @"
        /main.py:26:32 caller (function)
          call @ 40..43
        ");
    }

    #[test]
    fn incoming_non_call_reference_filtered_out() {
        let test = cursor_test(
            r#"
            def f<CURSOR>oo():
                pass

            def caller():
                cb = foo  # not a call — should NOT appear
                foo()     # this is a call — should appear once
            "#,
        );
        let incoming = test.incoming();
        // exactly one caller, with exactly one call-site range
        assert_eq!(incoming.len(), 1, "got {incoming:?}");
        assert_eq!(incoming[0].from_ranges.len(), 1);
    }

    #[test]
    fn incoming_multi_file() {
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
        let incoming = test.incoming();
        let names: Vec<_> = incoming
            .iter()
            .map(|c| c.from.name.as_str().to_string())
            .collect();
        assert!(names.contains(&"use".to_string()), "got callers: {names:?}");
    }

    #[test]
    fn incoming_via_import_alias() {
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
        let incoming = test.incoming();
        let names: Vec<_> = incoming
            .iter()
            .map(|c| c.from.name.as_str().to_string())
            .collect();
        assert!(
            names.contains(&"use".to_string()),
            "alias call should resolve through; got: {names:?}"
        );
    }

    #[test]
    fn incoming_keyword_call() {
        let test = cursor_test(
            r#"
            def f<CURSOR>oo(x):
                pass

            def caller():
                foo(x=1)
            "#,
        );
        let incoming = test.incoming();
        let total_sites: usize = incoming.iter().map(|c| c.from_ranges.len()).sum();
        assert!(total_sites >= 1, "got {incoming:?}");
    }

    #[test]
    fn incoming_top_level_call_attributed_to_module() {
        let test = cursor_test(
            r#"
            def f<CURSOR>oo():
                pass

            foo()
            "#,
        );
        let incoming = test.incoming();
        assert_eq!(incoming.len(), 1, "got {incoming:?}");
        assert_eq!(incoming[0].from.kind, CallHierarchyItemKind::Module);
    }

    #[test]
    fn incoming_decorator_application() {
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
        let incoming = test.incoming();
        let total_sites: usize = incoming.iter().map(|c| c.from_ranges.len()).sum();
        assert!(
            total_sites >= 1,
            "decorator should be recorded as call; got {incoming:?}"
        );
    }

    #[test]
    fn incoming_method_does_not_confuse_with_same_name_on_other_class() {
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
        let incoming = test.incoming();
        let total_sites: usize = incoming.iter().map(|c| c.from_ranges.len()).sum();
        // Should only record the `a.foo()` site, not `b.foo()`.
        assert_eq!(total_sites, 1, "got {incoming:?}");
    }

    #[test]
    fn incoming_super_method_call() {
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
        let incoming = test.incoming();
        let callers: Vec<_> = incoming.iter().map(|c| c.from.name.as_str()).collect();
        assert!(
            callers.contains(&"m"),
            "expected Child.m as a caller of Base.m, got: {callers:?}",
        );
    }

    // --- incoming: attribute-reference call sites --------------------------
    //
    // Beyond `ExprCall` and `Decorator`, an `ExprAttribute` reference can be
    // an implicit call: a `@property` access invokes the getter/setter/
    // deleter through the descriptor protocol; a bare bound-method reference
    // like `make_async(self.m, ...)` is the callee even though no parens
    // appear at the reference site.

    #[test]
    fn incoming_property_getter_read() {
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
        let incoming = test.incoming();
        let total_sites: usize = incoming.iter().map(|c| c.from_ranges.len()).sum();
        assert_eq!(
            total_sites, 1,
            "expected 1 getter call site; got {incoming:?}"
        );
    }

    #[test]
    fn incoming_property_setter_write() {
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
        let incoming = test.incoming();
        let total_sites: usize = incoming.iter().map(|c| c.from_ranges.len()).sum();
        assert_eq!(
            total_sites, 1,
            "expected setter to match the write site only; got {incoming:?}",
        );
        // And the matched caller must be `write`, not `read`.
        let names: Vec<_> = incoming
            .iter()
            .map(|c| c.from.name.as_str().to_string())
            .collect();
        assert!(names.contains(&"write".to_string()), "got: {names:?}");
    }

    #[test]
    fn incoming_property_deleter_del() {
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
        let incoming = test.incoming();
        let total_sites: usize = incoming.iter().map(|c| c.from_ranges.len()).sum();
        assert_eq!(
            total_sites, 1,
            "expected deleter to match the del site only; got {incoming:?}",
        );
        let names: Vec<_> = incoming
            .iter()
            .map(|c| c.from.name.as_str().to_string())
            .collect();
        assert!(names.contains(&"remove".to_string()), "got: {names:?}");
    }

    #[test]
    fn incoming_method_reference_passed_as_arg() {
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
        let incoming = test.incoming();
        let total_sites: usize = incoming.iter().map(|c| c.from_ranges.len()).sum();
        assert!(
            total_sites >= 1,
            "expected `self.method` arg reference to be a call site; got {incoming:?}",
        );
    }

    #[test]
    fn incoming_method_reference_assigned() {
        let test = cursor_test(
            r#"
            class C:
                def metho<CURSOR>d(self) -> int:
                    return 1

                def setup(self) -> None:
                    cb = self.method
            "#,
        );
        let incoming = test.incoming();
        let total_sites: usize = incoming.iter().map(|c| c.from_ranges.len()).sum();
        assert!(
            total_sites >= 1,
            "expected `cb = self.method` to record method as call site; got {incoming:?}",
        );
    }

    #[test]
    fn incoming_attribute_call_not_double_counted() {
        // `c.method()` is one site, not two — the `ExprCall` arm and the
        // `ExprAttribute` arm must not both record the same range.
        let test = cursor_test(
            r#"
            class C:
                def metho<CURSOR>d(self) -> int:
                    return 1

            def use(c: C) -> int:
                return c.method()
            "#,
        );
        let incoming = test.incoming();
        let total_sites: usize = incoming.iter().map(|c| c.from_ranges.len()).sum();
        assert_eq!(
            total_sites, 1,
            "expected exactly one site for c.method(); got {incoming:?}",
        );
    }

    #[test]
    fn incoming_non_callable_attribute_filtered() {
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
        let incoming = test.incoming();
        // `c.func` resolves to the `int` instance attribute, not the free
        // function `func`, so it must not be in `func`'s callers — and the
        // only legitimate `func` use in the fixture (`self.func = 42`) is a
        // write to an instance attribute, never a call.
        let source = test.cursor.source.as_str().to_string();
        for call in &incoming {
            for range in &call.from_ranges {
                let start = range.start().to_usize();
                let end = range.end().to_usize();
                let text = &source[start..end];
                assert_ne!(
                    text, "func",
                    "stray match on c.func from caller {}: {call:?}",
                    call.from.name,
                );
            }
        }
    }

    #[test]
    fn incoming_lambda_caller_is_synthesized_item() {
        // A call inside a top-level lambda should be attributed to a
        // synthetic `(lambda)` item, not to the module.
        let test = cursor_test(
            r#"
            def tar<CURSOR>get(x):
                pass

            f = lambda x: target(x)
            "#,
        );
        let incoming = test.incoming();
        assert_eq!(incoming.len(), 1, "got {incoming:?}");
        assert_eq!(incoming[0].from.name.as_str(), "(lambda)");
        assert_eq!(incoming[0].from.kind, CallHierarchyItemKind::Function);
        // selection_range must anchor at the `lambda` keyword (6 chars).
        let sel = incoming[0].from.selection_range;
        let source = test.cursor.source.as_str();
        assert_eq!(
            &source[sel.start().to_usize()..sel.end().to_usize()],
            "lambda",
        );
        assert_eq!(incoming[0].from_ranges.len(), 1);
    }

    #[test]
    fn incoming_two_lambdas_calling_same_function_two_distinct_items() {
        // Two separate lambdas, both calling `target`, must surface as
        // two distinct `(lambda)` items with different selection_ranges
        // — not collapsed into one entry.
        let test = cursor_test(
            r#"
            def tar<CURSOR>get(x):
                pass

            a = lambda x: target(x)
            b = lambda y: target(y)
            "#,
        );
        let incoming = test.incoming();
        assert_eq!(incoming.len(), 2, "got {incoming:?}");
        for call in &incoming {
            assert_eq!(call.from.name.as_str(), "(lambda)");
            assert_eq!(call.from.kind, CallHierarchyItemKind::Function);
            assert_eq!(call.from_ranges.len(), 1);
        }
        assert_ne!(
            incoming[0].from.selection_range,
            incoming[1].from.selection_range,
        );
    }

    #[test]
    fn incoming_lambda_inside_function_attributed_to_lambda() {
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
        let incoming = test.incoming();
        assert_eq!(incoming.len(), 1, "got {incoming:?}");
        assert_eq!(incoming[0].from.name.as_str(), "(lambda)");
        assert_eq!(incoming[0].from.kind, CallHierarchyItemKind::Function);
    }

    #[test]
    fn incoming_comprehension_attributed_to_enclosing_function() {
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
        let incoming = test.incoming();
        assert_eq!(incoming.len(), 1, "got {incoming:?}");
        assert_eq!(incoming[0].from.name.as_str(), "caller");
        assert_eq!(incoming[0].from.kind, CallHierarchyItemKind::Function);
    }

    #[test]
    fn lambda_follow_up_requests_are_leaves() {
        // Round-trip guard: the synthetic `(lambda)` item surfaced as a
        // caller has a `selection_range` anchored at the `lambda` keyword.
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
        let incoming = test.incoming();
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
