use rustc_hash::FxHashMap;

use ast::traversal;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::traversal::EnclosingSuite;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_codegen::Generator;
use ruff_python_semantic::analyze::typing::is_list;
use ruff_python_semantic::{Binding, BindingId, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for consecutive calls to `append`.
///
/// ## Why is this bad?
/// Consecutive calls to `append` can be less efficient than batching them into
/// a single `extend`. Each `append` resizes the list individually, whereas an
/// `extend` can resize the list once for all elements.
///
/// ## Known problems
/// This rule is prone to false negatives due to type inference limitations,
/// as it will only detect lists that are instantiated as literals or annotated
/// with a type annotation.
///
/// ## Example
/// ```python
/// nums = [1, 2, 3]
///
/// nums.append(4)
/// nums.append(5)
/// nums.append(6)
/// ```
///
/// Use instead:
/// ```python
/// nums = [1, 2, 3]
///
/// nums.extend((4, 5, 6))
/// ```
///
/// ## References
/// - [Python documentation: More on Lists](https://docs.python.org/3/tutorial/datastructures.html#more-on-lists)
#[derive(ViolationMetadata)]
pub(crate) struct RepeatedAppend {
    name: String,
    replacement: SourceCodeSnippet,
}

impl RepeatedAppend {
    fn suggestion(&self) -> String {
        let name = &self.name;
        self.replacement
            .full_display()
            .map_or(format!("{name}.extend(...)"), ToString::to_string)
    }
}

impl Violation for RepeatedAppend {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let name = &self.name;
        let suggestion = self.suggestion();
        format!("Use `{suggestion}` instead of repeatedly calling `{name}.append()`")
    }

    fn fix_title(&self) -> Option<String> {
        let suggestion = self.suggestion();
        Some(format!("Replace with `{suggestion}`"))
    }
}

/// FURB113
pub(crate) fn repeated_append(checker: &Checker, stmt: &Stmt) {
    let Some(appends) = match_consecutive_appends(stmt, checker.semantic()) else {
        return;
    };

    // No need to proceed if we have less than 1 `append` to work with.
    if appends.len() <= 1 {
        return;
    }

    // group borrows from checker, so we can't directly push into checker.diagnostics
    let diagnostics: Vec<Diagnostic> = group_appends(appends)
        .iter()
        .filter_map(|group| {
            // Groups with just one element are fine, and shouldn't be replaced by `extend`.
            if group.appends.len() <= 1 {
                return None;
            }

            let replacement = make_suggestion(group, checker.generator());

            let mut diagnostic = Diagnostic::new(
                RepeatedAppend {
                    name: group.name().to_string(),
                    replacement: SourceCodeSnippet::new(replacement.clone()),
                },
                group.range(),
            );

            // We only suggest a fix when all appends in a group are clumped together. If they're
            // non-consecutive, fixing them is much more difficult.
            //
            // Avoid fixing if there are comments in between the appends:
            //
            // ```python
            // a.append(1)
            // # comment
            // a.append(2)
            // ```
            if group.is_consecutive && !checker.comment_ranges().intersects(group.range()) {
                diagnostic.set_fix(Fix::unsafe_edit(Edit::replacement(
                    replacement,
                    group.start(),
                    group.end(),
                )));
            }

            Some(diagnostic)
        })
        .collect();

    checker.report_diagnostics(diagnostics);
}

#[derive(Debug, Clone)]
struct Append<'a> {
    /// Receiver of the `append` call (aka `self` argument).
    receiver: &'a ast::ExprName,
    /// [`BindingId`] that the receiver references.
    binding_id: BindingId,
    /// [`Binding`] that the receiver references.
    binding: &'a Binding<'a>,
    /// [`Expr`] serving as a sole argument to `append`.
    argument: &'a Expr,
    /// The statement containing the `append` call.
    stmt: &'a Stmt,
}

#[derive(Debug)]
struct AppendGroup<'a> {
    /// A sequence of `appends` connected to the same binding.
    appends: Vec<Append<'a>>,
    /// `true` when all appends in the group follow one another and don't have other statements in
    /// between. It is much easier to make fix suggestions for consecutive groups.
    is_consecutive: bool,
}

impl AppendGroup<'_> {
    fn name(&self) -> &str {
        assert!(!self.appends.is_empty());
        &self.appends.first().unwrap().receiver.id
    }
}

impl Ranged for AppendGroup<'_> {
    fn range(&self) -> TextRange {
        assert!(!self.appends.is_empty());
        TextRange::new(
            self.appends.first().unwrap().stmt.start(),
            self.appends.last().unwrap().stmt.end(),
        )
    }
}

/// Match consecutive calls to `append` on list variables starting from the given statement.
fn match_consecutive_appends<'a>(
    stmt: &'a Stmt,
    semantic: &'a SemanticModel,
) -> Option<Vec<Append<'a>>> {
    // Match the current statement, to see if it's an append.
    let append = match_append(semantic, stmt)?;

    // In order to match consecutive statements, we need to go to the tree ancestor of the
    // given statement, find its position there, and match all 'appends' from there.
    let suite = if semantic.at_top_level() {
        // If the statement is at the top level, we should go to the parent module.
        // Module is available in the definitions list.
        EnclosingSuite::new(semantic.definitions.python_ast()?, stmt)?
    } else {
        // Otherwise, go to the parent, and take its body as a sequence of siblings.
        semantic
            .current_statement_parent()
            .and_then(|parent| traversal::suite(stmt, parent))?
    };

    // We shouldn't repeat the same work for many 'appends' that go in a row. Let's check
    // that this statement is at the beginning of such a group.
    if suite
        .previous_sibling()
        .is_some_and(|previous_stmt| match_append(semantic, previous_stmt).is_some())
    {
        return None;
    }

    // Starting from the next statement, let's match all appends and make a vector.
    Some(
        std::iter::once(append)
            .chain(
                suite
                    .next_siblings()
                    .iter()
                    .map_while(|sibling| match_append(semantic, sibling)),
            )
            .collect(),
    )
}

/// Group the given appends by the associated bindings.
fn group_appends(appends: Vec<Append<'_>>) -> Vec<AppendGroup<'_>> {
    // We want to go over the given list of appends and group the by receivers.
    let mut map: FxHashMap<BindingId, AppendGroup> = FxHashMap::default();
    let mut iter = appends.into_iter();
    let mut last_binding = {
        let first_append = iter.next().unwrap();
        let binding_id = first_append.binding_id;
        let _ = get_or_add(&mut map, first_append);
        binding_id
    };

    for append in iter {
        let binding_id = append.binding_id;
        let group = get_or_add(&mut map, append);
        if binding_id != last_binding {
            // If the group is not brand new, and the previous group was different,
            // we should mark it as "non-consecutive".
            //
            // We are catching the following situation:
            // ```python
            // a.append(1)
            // a.append(2)
            // b.append(1)
            // a.append(3) # <- we are currently here
            // ```
            //
            // So, `a` != `b` and group for `a` already contains appends 1 and 2.
            // It is only possible if this group got interrupted by at least one
            // other group and, thus, it is non-consecutive.
            if group.appends.len() > 1 {
                group.is_consecutive = false;
            }

            last_binding = binding_id;
        }
    }

    map.into_values().collect()
}

#[inline]
fn get_or_add<'a, 'b>(
    map: &'b mut FxHashMap<BindingId, AppendGroup<'a>>,
    append: Append<'a>,
) -> &'b mut AppendGroup<'a> {
    let group = map.entry(append.binding_id).or_insert(AppendGroup {
        appends: vec![],
        is_consecutive: true,
    });
    group.appends.push(append);
    group
}

/// Matches that the given statement is a call to `append` on a list variable.
fn match_append<'a>(semantic: &'a SemanticModel, stmt: &'a Stmt) -> Option<Append<'a>> {
    let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt else {
        return None;
    };

    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = value.as_ref()
    else {
        return None;
    };

    // `append` should have just one argument, an element to be added.
    let [argument] = &*arguments.args else {
        return None;
    };

    // The called function should be an attribute, ie `value.attr`.
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
        return None;
    };

    // `attr` should be `append` and it shouldn't have any keyword arguments.
    if attr != "append" || !arguments.keywords.is_empty() {
        return None;
    }

    // We match only variable references, i.e. `value` should be a name expression.
    let Expr::Name(receiver @ ast::ExprName { id: name, .. }) = value.as_ref() else {
        return None;
    };

    // Now we need to find what is this variable bound to...
    let scope = semantic.current_scope();
    let bindings: Vec<BindingId> = scope.get_all(name).collect();

    // Maybe it is too strict of a limitation, but it seems reasonable.
    let [binding_id] = bindings.as_slice() else {
        return None;
    };

    let binding = semantic.binding(*binding_id);

    // ...and whether this something is a list.
    if !is_list(binding, semantic) {
        return None;
    }

    Some(Append {
        receiver,
        binding_id: *binding_id,
        binding,
        stmt,
        argument,
    })
}

/// Make fix suggestion for the given group of appends.
fn make_suggestion(group: &AppendGroup, generator: Generator) -> String {
    let appends = &group.appends;

    assert!(!appends.is_empty());
    let first = appends.first().unwrap();

    assert!(appends
        .iter()
        .all(|append| append.binding.source == first.binding.source));

    // Here we construct `var.extend((elt1, elt2, ..., eltN))
    //
    // Each eltK comes from an individual `var.append(eltK)`.
    let elts: Vec<Expr> = appends
        .iter()
        .map(|append| append.argument.clone())
        .collect();
    // Join all elements into a tuple: `(elt1, elt2, ..., eltN)`
    let tuple = ast::ExprTuple {
        elts,
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
        parenthesized: true,
    };
    // Make `var.extend`.
    // NOTE: receiver is the same for all appends and that's why we can take the first.
    let attr = ast::ExprAttribute {
        value: Box::new(first.receiver.clone().into()),
        attr: ast::Identifier::new("extend".to_string(), TextRange::default()),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    // Make the actual call `var.extend((elt1, elt2, ..., eltN))`
    let call = ast::ExprCall {
        func: Box::new(attr.into()),
        arguments: ast::Arguments {
            args: Box::from([tuple.into()]),
            keywords: Box::from([]),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    // And finally, turn it into a statement.
    let stmt = ast::StmtExpr {
        value: Box::new(call.into()),
        range: TextRange::default(),
    };
    generator.stmt(&stmt.into())
}
