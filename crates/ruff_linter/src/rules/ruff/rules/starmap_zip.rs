use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprCall};
use ruff_python_parser::TokenKind;
use ruff_text_size::{Ranged, TextRange, TextSize};
use smallvec::{smallvec, SmallVec};

/// ## What it does
/// Checks for `itertools.starmap` calls where the second argument is a `zip` call.
///
/// ## Why is this bad?
/// `starmap` should only be used for pre-zipped iterables.
///
/// ## Example
///
/// ```python
/// from itertools import starmap
///
///
/// starmap(func, zip(a, b))
/// starmap(func, zip(a, b, strict=True))
/// ```
///
/// Use instead:
///
/// ```python
/// map(func, a, b)
/// map(func, a, b, strict=True)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct StarmapZip;

impl AlwaysFixableViolation for StarmapZip {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`itertools.starmap` called on `zip` iterable".to_string()
    }

    fn fix_title(&self) -> String {
        "Use `map` instead".to_string()
    }
}

/// RUF058
pub(crate) fn starmap_zip(checker: &mut Checker, call: &ExprCall) {
    let semantic = checker.semantic();

    let Some(qualified_name) = semantic.resolve_qualified_name(&call.func) else {
        return;
    };

    if !matches!(qualified_name.segments(), ["itertools", "starmap"]) {
        return;
    }

    if !call.arguments.keywords.is_empty() {
        return;
    }

    let [map_func, Expr::Call(iterable_call)] = call.arguments.args.as_ref() else {
        return;
    };

    if !semantic.match_builtin_expr(&iterable_call.func, "zip") {
        return;
    }

    if !iterable_call.arguments.keywords.is_empty() {
        // TODO: Pass `strict=` to `map` too when 3.14 is supported.
        return;
    }

    let fix = replace_with_map(call, map_func, iterable_call, checker);
    let diagnostic = Diagnostic::new(StarmapZip, call.range);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

fn replace_with_map(starmap: &ExprCall, map_func: &Expr, zip: &ExprCall, checker: &Checker) -> Fix {
    let starmap_func_range = TextRange::new(starmap.start(), starmap.arguments.start());
    let change_func_to_map = Edit::range_replacement("map".to_string(), starmap_func_range);

    let mut remove_zip: SmallVec<[Edit; 4]> = smallvec![];

    let zip_start_range = TextRange::new(
        zip.start(),
        zip.arguments.start() + TextSize::try_from("(".len()).unwrap(),
    );
    remove_zip.push(Edit::range_deletion(zip_start_range));

    if zip.arguments.is_empty() {
        remove_zip.push(Edit::insertion("[]".to_string(), zip_start_range.end()));
    }

    let map_func_end = checker
        .tokens()
        .after(map_func.end())
        .iter()
        .find(|token| matches!(token.kind(), TokenKind::Comma))
        .unwrap()
        .end();
    let mut open_parens = 0_usize;

    for token in checker
        .tokens()
        .in_range(TextRange::new(map_func_end, zip.start()))
    {
        match token.kind() {
            kind if kind.is_trivia() => {}
            TokenKind::Lpar => {
                remove_zip.push(Edit::range_deletion(token.range()));
                open_parens += 1;
            }
            _ => {
                break;
            }
        }
    }

    let zip_end = zip.arguments.end();
    let before_zip_end = zip_end - TextSize::try_from(")".len()).unwrap();
    remove_zip.push(Edit::deletion(before_zip_end, zip_end));

    for token in checker.tokens().after(zip_end) {
        match token.kind() {
            TokenKind::Rpar if open_parens > 0 => {
                remove_zip.push(Edit::range_deletion(token.range()));
                open_parens -= 1;
            }
            TokenKind::Rpar => {
                break;
            }
            TokenKind::Comma => {
                remove_zip.push(Edit::range_deletion(token.range()));
                break;
            }
            _ => {}
        }
    }

    let comment_ranges = checker.comment_ranges();

    let applicability = if comment_ranges.intersects(starmap_func_range)
        || comment_ranges.intersects(zip_start_range)
    {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    Fix::applicable_edits(change_func_to_map, remove_zip, applicability)
}
