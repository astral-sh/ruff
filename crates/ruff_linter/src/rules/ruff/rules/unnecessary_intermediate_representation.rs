use crate::checkers::ast::Checker;
use crate::Locator;
use itertools::Itertools;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    Arguments, DictItem, Expr, ExprAttribute, ExprCall, ExprName, ExprStarred, ExprStringLiteral,
    Operator, StmtAugAssign, StringLiteral, StringLiteralFlags, StringLiteralValue,
};
use ruff_python_semantic::analyze::typing::{is_dict, is_list, is_set};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange, TextSize};

/// ## What it does
/// Checks for situations where collections are unnecessarily created
/// and then immediately discarded.
///
/// ## Why is this bad?
/// Such collections may cause the code to be less performant.
/// Use tuples instead, as they can be evaluated at compile time.
///
/// ## Known problems
/// This rule is prone to false negatives due to type inference limitations.
/// Only variables instantiated as literals or have type annotations are detected correctly.
///
/// ## Example
///
/// ```python
/// lst = []
/// lst += ["value"]
/// lst += ["value 1", "value 2"]
///
/// s = set()
/// s |= {"value"}
/// s |= {"value 1", "value 2"}
///
/// d = {}
/// d |= {"key": "value"}
/// ```
///
/// Use instead:
///
/// ```python
/// lst = []
/// lst.append("value")
/// lst.extend(("value 1", "value 2"))
///
/// s = set()
/// s.add("value")
/// s.update(("value 1", "value 2"))
///
/// d = {}
/// d["key"] = "value"
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryIntermediateRepresentation {
    fix_title: String,
}

impl AlwaysFixableViolation for UnnecessaryIntermediateRepresentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary intermediate representation".to_string()
    }

    fn fix_title(&self) -> String {
        self.fix_title.to_string()
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, is_macro::Is)]
enum IterableKind {
    Tuple,
    List,
    Set,
    Dict,
}

impl IterableKind {
    fn from_target(semantic: &SemanticModel, name: &ExprName) -> Option<IterableKind> {
        let binding_id = semantic.only_binding(name)?;
        let binding = semantic.binding(binding_id);

        match () {
            () if is_list(binding, semantic) => Some(IterableKind::List),
            () if is_set(binding, semantic) => Some(IterableKind::Set),
            () if is_dict(binding, semantic) => Some(IterableKind::Dict),
            () => None,
        }
    }
}

struct Iterable {
    kind: IterableKind,
    elements: Vec<Element>,
}

impl Iterable {
    fn len(&self) -> usize {
        self.elements.len()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn has_unpack(&self) -> bool {
        self.elements.iter().any(Element::is_unpack)
    }

    fn element_exprs(&self, locator: &Locator) -> String {
        let elements = &self.elements;

        if self.kind.is_dict() {
            return elements.iter().map(|it| it.expr(locator)).join(", ");
        }

        let start = elements.first().unwrap().start();
        let end = elements.last().unwrap().end();

        locator.slice(TextRange::new(start, end)).to_string()
    }
}

impl Iterable {
    fn from(expr: &Expr) -> Option<Self> {
        let (kind, elements) = match expr {
            Expr::Tuple(tuple) => (
                IterableKind::Tuple,
                tuple.iter().map(Element::from).collect_vec(),
            ),
            Expr::List(list) => (
                IterableKind::List,
                list.iter().map(Element::from).collect_vec(),
            ),
            Expr::Set(set) => (
                IterableKind::Set,
                set.iter().map(Element::from).collect_vec(),
            ),
            Expr::Dict(dict) => (
                IterableKind::Dict,
                dict.iter().map(Element::from).collect_vec(),
            ),
            _ => return None,
        };

        Some(Self { kind, elements })
    }

    fn from_single(arguments: &[Expr]) -> Option<Self> {
        let [single] = arguments else {
            return None;
        };

        Self::from(single)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Element {
    /// `"foo"`, `bar`, `42`, etc.
    Single(TextRange),
    /// `*baz`
    StarUnpack(TextRange),
    /// `**qux`
    DictUnpack(TextRange),
    /// `lorem: ipsum`
    DictItem { key: TextRange, value: TextRange },
}

impl Element {
    fn is_unpack(&self) -> bool {
        matches!(self, Self::StarUnpack(..) | Self::DictUnpack(..))
    }

    /// This element as it is expressed in the source code.
    ///
    /// For a [`Self::DictItem`], only the key is returned.
    fn expr(&self, locator: &Locator) -> String {
        let in_source = locator.slice(self);

        match self {
            Self::Single(..) => in_source.to_string(),
            Self::DictItem { key, .. } => locator.slice(key).to_string(),
            Self::StarUnpack(..) => format!("*({in_source})"),
            Self::DictUnpack(..) => format!("**({in_source})"),
        }
    }
}

impl From<&Expr> for Element {
    fn from(expr: &Expr) -> Self {
        match expr {
            Expr::Starred(ExprStarred { value, .. }) => Self::StarUnpack(value.range()),
            _ => Self::Single(expr.range()),
        }
    }
}

impl From<&DictItem> for Element {
    fn from(item: &DictItem) -> Self {
        let key = item.key.as_ref().map(Ranged::range);
        let value = item.value.range();

        match key {
            None => Element::DictUnpack(value),
            Some(key) => Element::DictItem { key, value },
        }
    }
}

impl Ranged for Element {
    fn range(&self) -> TextRange {
        match self {
            Element::Single(range) => *range,
            Element::StarUnpack(range) => *range,
            Element::DictUnpack(range) => *range,
            Element::DictItem { key, value } => TextRange::new(key.start(), value.end()),
        }
    }

    fn start(&self) -> TextSize {
        self.range().start()
    }

    fn end(&self) -> TextSize {
        self.range().end()
    }
}

fn create_string_expr(content: String) -> Expr {
    let range = TextRange::default();

    let literal = StringLiteral {
        range,
        value: Box::from(content),
        flags: StringLiteralFlags::default(),
    };

    let value = StringLiteralValue::single(literal);

    Expr::StringLiteral(ExprStringLiteral { range, value })
}

/// Returns a [`Diagnostic`] whose fix suggests replacing `range`
/// with `{target}.{method}({element})` (one pair of parentheses).
fn use_single_arg_method_call(
    range: TextRange,
    target: &str,
    method: &str,
    element: &str,
) -> Diagnostic {
    let new_content = format!("{target}.{method}({element})");
    let edit = Edit::range_replacement(new_content, range);
    let fix = Fix::safe_edit(edit);

    let fix_title = format!("Replace with `.{method}()`");
    let kind = UnnecessaryIntermediateRepresentation { fix_title };
    let diagnostic = Diagnostic::new(kind, range);

    diagnostic.with_fix(fix)
}

/// Returns a [`Diagnostic`] whose fix suggests replacing `range`
/// with `{target}.{method}(({element}))` (two pairs of parentheses).
fn use_tuple_arg_method_call(
    range: TextRange,
    target: &str,
    method: &str,
    element: &str,
) -> Diagnostic {
    let new_content = format!("{target}.{method}(({element}))");
    let edit = Edit::range_replacement(new_content, range);
    let fix = Fix::safe_edit(edit);

    let fix_title = format!("Replace with `.{method}()`");
    let kind = UnnecessaryIntermediateRepresentation { fix_title };
    let diagnostic = Diagnostic::new(kind, range);

    diagnostic.with_fix(fix)
}

/// Returns a [`Diagnostic`] whose fix suggests replacing `range`
/// with `{target}[{key}] = {value}`.
fn use_item_assignment(range: TextRange, target: &str, value: &str, key: &str) -> Diagnostic {
    let new_content = format!("{target}[{key}] = {value}");
    let edit = Edit::range_replacement(new_content, range);
    let fix = Fix::safe_edit(edit);

    let fix_title = "Replace with item assignment".to_string();
    let kind = UnnecessaryIntermediateRepresentation { fix_title };
    let diagnostic = Diagnostic::new(kind, range);

    diagnostic.with_fix(fix)
}

/// RUF042: Method calls
pub(crate) fn unnecessary_intermediate_representation_call(checker: &mut Checker, call: &ExprCall) {
    let (func, arguments) = (&call.func, &call.arguments);
    let range = call.range;

    let Expr::Attribute(ExprAttribute { value, attr, .. }) = func.as_ref() else {
        return;
    };
    let Expr::Name(target) = value.as_ref() else {
        return;
    };
    let Some(target_kind) = IterableKind::from_target(checker.semantic(), target) else {
        return;
    };

    let (positionals, keywords) = (&arguments.args, &arguments.keywords);
    let in_stmt_context = checker.semantic().current_expression_parent().is_none();

    match (target_kind, attr.as_str()) {
        (IterableKind::List, "extend") if keywords.is_empty() => {
            let Some(iterable) = Iterable::from_single(positionals) else {
                return;
            };

            list_extend_single(checker, range, target, &iterable);
        }

        (IterableKind::Set, method @ ("update" | "difference_update")) => {
            let Some(iterable) = Iterable::from_single(positionals) else {
                return;
            };

            set_update_single(checker, range, target, method, &iterable);
        }

        (IterableKind::Dict, "update") if !keywords.is_empty() && in_stmt_context => {
            if !positionals.is_empty() {
                return;
            }

            let [argument] = &keywords[..] else {
                return;
            };

            if argument.arg.is_none() {
                let element = Element::DictUnpack(argument.value.range());
                dict_update_unpack(checker, range, target, &element);
            } else {
                dict_update_single_keyword(checker, range, target, arguments);
            }
        }

        (IterableKind::Dict, "update") if in_stmt_context => {
            let Some(iterable) = Iterable::from_single(positionals) else {
                return;
            };
            let [element] = &iterable.elements[..] else {
                return;
            };

            if element.is_unpack() {
                dict_update_single_unpack(checker, range, target, element);
            } else {
                dict_update_single(checker, range, target, element);
            }
        }

        _ => {}
    };
}

/// * `l.extend(["foo"])` -> `l.append("foo")`
/// * `l.extend([*foo])` -> `l.extend(foo)`
fn list_extend_single(
    checker: &mut Checker,
    range: TextRange,
    target: &ExprName,
    iterable: &Iterable,
) {
    list_iadd_single(checker, range, target, iterable);
}

/// * `s.update({"foo"})` -> `s.add("foo")`
/// * `s.difference_update({"foo"})` -> `s.discard("foo")`
fn set_update_single(
    checker: &mut Checker,
    range: TextRange,
    target: &ExprName,
    method: &str,
    iterable: &Iterable,
) {
    let op = match method {
        "update" => Operator::BitOr,
        "difference_update" => Operator::Sub,
        _ => return,
    };

    set_iop_single(checker, range, target, op, iterable);
}

/// `d.update({"foo": "bar"})` -> `d["foo"] = "bar"`
fn dict_update_single(
    checker: &mut Checker,
    range: TextRange,
    target: &ExprName,
    element: &Element,
) {
    dict_ior_single(checker, range, target, element);
}

/// `d.update({**foo})` -> `d.update(foo)`
fn dict_update_single_unpack(
    checker: &mut Checker,
    range: TextRange,
    target: &ExprName,
    element: &Element,
) {
    let locator = checker.locator();
    let target_expr = locator.slice(target);
    let element_expr = locator.slice(element);

    let method = "update";
    let diagnostic = use_single_arg_method_call(range, target_expr, method, element_expr);

    checker.diagnostics.push(diagnostic);
}

/// `d.update(**foo)` -> `d.update(foo)`
fn dict_update_unpack(
    checker: &mut Checker,
    range: TextRange,
    target: &ExprName,
    element: &Element,
) {
    let locator = checker.locator();
    let target_expr = locator.slice(target);
    let element_expr = locator.slice(element);

    let method = "update";
    let diagnostic = use_single_arg_method_call(range, target_expr, method, element_expr);

    checker.diagnostics.push(diagnostic);
}

/// `d.update(foo="bar")` -> `d["foo"] = "bar"`
fn dict_update_single_keyword(
    checker: &mut Checker,
    range: TextRange,
    target: &ExprName,
    arguments: &Arguments,
) {
    if !arguments.args.is_empty() {
        return;
    }

    let [argument] = &arguments.keywords[..] else {
        return;
    };
    let (Some(name), value) = (&argument.arg, &argument.value) else {
        return;
    };

    let locator = checker.locator();
    let generator = checker.generator();

    let target_expr = locator.slice(target);
    let value_expr = locator.slice(value);
    let key_expr = generator.expr(&create_string_expr(name.to_string()));

    let diagnostic = use_item_assignment(range, target_expr, value_expr, &key_expr);

    checker.diagnostics.push(diagnostic);
}

/// RUF042: Augmented assignments
pub(crate) fn unnecessary_intermediate_representation_aug_assign(
    checker: &mut Checker,
    aug_assign: &StmtAugAssign,
) {
    let (target, op, value) = (&aug_assign.target, aug_assign.op, &aug_assign.value);
    let range = aug_assign.range;

    let Expr::Name(target) = target.as_ref() else {
        return;
    };

    let Some(target_kind) = IterableKind::from_target(checker.semantic(), target) else {
        return;
    };
    let Some(iterable) = Iterable::from(value) else {
        return;
    };

    match (target_kind, op, iterable.kind) {
        (IterableKind::List, Operator::Add, _) => {
            if iterable.len() == 1 {
                list_iadd_single(checker, range, target, &iterable);
            } else {
                list_iadd_multiple(checker, range, target, &iterable);
            }
        }

        (IterableKind::Set, _, IterableKind::Set) => {
            if iterable.len() == 1 {
                set_iop_single(checker, range, target, op, &iterable);
            } else {
                set_iop_multiple(checker, range, op, target, &iterable);
            }
        }

        (IterableKind::Dict, Operator::BitOr, IterableKind::Dict) => {
            let [element] = &iterable.elements[..] else {
                return;
            };

            if element.is_unpack() {
                dict_ior_single_unpack(checker, range, target, element);
            } else {
                dict_ior_single(checker, range, target, element);
            };
        }

        _ => {}
    };
}

/// * `l += [foo]` -> `l.append(foo)`
/// * `l += [*foo]` -> `l.extend(foo)`
fn list_iadd_single(
    checker: &mut Checker,
    range: TextRange,
    target: &ExprName,
    iterable: &Iterable,
) {
    let [element] = &iterable.elements[..] else {
        return;
    };

    if element.is_unpack() && !(iterable.kind.is_list() || iterable.kind.is_tuple()) {
        return;
    }

    let locator = checker.locator();
    let target_expr = locator.slice(target);

    let (method, element_expr) = if element.is_unpack() {
        ("extend", locator.slice(element))
    } else {
        ("append", &*element.expr(locator))
    };

    let diagnostic = use_single_arg_method_call(range, target_expr, method, element_expr);

    checker.diagnostics.push(diagnostic);
}

/// * `l += ["foo", "bar"]` -> `l.extend(("foo", "bar"))`
fn list_iadd_multiple(
    checker: &mut Checker,
    range: TextRange,
    target: &ExprName,
    iterable: &Iterable,
) {
    if iterable.is_empty() || iterable.has_unpack() {
        return;
    }

    let locator = checker.locator();
    let target_expr = locator.slice(target);
    let element_exprs = iterable.element_exprs(locator);

    let method = "extend";
    let diagnostic = use_tuple_arg_method_call(range, target_expr, method, &element_exprs);

    checker.diagnostics.push(diagnostic);
}

/// * `s |= {"foo"}` -> `s.add("foo")`
/// * `s -= {"foo"}` -> `s.discard("foo")`
/// * `s |= {*foo}` -> `s.update(foo)`
/// * `s -= {*foo}` -> `s.difference_update(foo)`
fn set_iop_single(
    checker: &mut Checker,
    range: TextRange,
    target: &ExprName,
    op: Operator,
    iterable: &Iterable,
) {
    let [element] = &iterable.elements[..] else {
        return;
    };

    let locator = checker.locator();
    let target_expr = locator.slice(target);

    let (method, element_expr) = match (op, element.is_unpack()) {
        (Operator::BitOr, true) => ("update", locator.slice(element)),
        (Operator::Sub, true) => ("difference_update", locator.slice(element)),

        (Operator::BitOr, false) => ("add", &*element.expr(locator)),
        (Operator::Sub, false) => ("discard", &*element.expr(locator)),

        _ => return,
    };

    let diagnostic = use_single_arg_method_call(range, target_expr, method, element_expr);

    checker.diagnostics.push(diagnostic);
}

/// * `s |= {"foo", "bar"}` -> `s.update(("foo", "bar"))`
/// * `s &= {"foo", "bar"}` -> `s.intersection_update(("foo", "bar"))`
/// * `s -= {"foo", "bar"}` -> `s.difference_update(("foo", "bar"))`
/// * `s ^= {"foo", "bar"}` -> `s.symmetric_difference_update(("foo", "bar"))`
fn set_iop_multiple(
    checker: &mut Checker,
    range: TextRange,
    op: Operator,
    target: &ExprName,
    iterable: &Iterable,
) {
    if iterable.is_empty() || iterable.has_unpack() {
        return;
    }

    let method = match op {
        Operator::Sub => "difference_update",
        Operator::BitOr => "update",
        Operator::BitXor => "symmetric_difference_update",
        Operator::BitAnd => "intersection_update",
        _ => return,
    };

    let locator = checker.locator();
    let target_expr = locator.slice(target);
    let element_exprs = iterable.element_exprs(locator);

    let diagnostic = use_tuple_arg_method_call(range, target_expr, method, &element_exprs);

    checker.diagnostics.push(diagnostic);
}

/// * `d |= {"foo": "bar"}` -> `d["foo"] = "bar"`
fn dict_ior_single(checker: &mut Checker, range: TextRange, target: &ExprName, element: &Element) {
    let Element::DictItem { key, value } = element else {
        return;
    };

    let locator = checker.locator();
    let target_expr = locator.slice(target);
    let key_expr = locator.slice(key);
    let value_expr = locator.slice(value);

    let diagnostic = use_item_assignment(range, target_expr, key_expr, value_expr);

    checker.diagnostics.push(diagnostic);
}

/// * `d |= {**unpack}` -> `d.update(unpack)`
fn dict_ior_single_unpack(
    checker: &mut Checker,
    range: TextRange,
    target: &ExprName,
    element: &Element,
) {
    let locator = checker.locator();
    let target_expr = locator.slice(target);
    let element_expr = locator.slice(element);

    let method = "update";
    let diagnostic = use_single_arg_method_call(range, target_expr, method, element_expr);

    checker.diagnostics.push(diagnostic);
}
