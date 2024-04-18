use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::flake8_comprehensions::fixes;
use crate::rules::flake8_comprehensions::fixes::{pad_end, pad_start};
use crate::rules::flake8_comprehensions::settings::Settings;

/// ## What it does
/// Checks for unnecessary `dict`, `list` or `tuple` calls that can be
/// rewritten as empty literals.
///
/// ## Why is this bad?
/// It's unnecessary to call, e.g., `dict()` as opposed to using an empty
/// literal (`{}`). The former is slower because the name `dict` must be
/// looked up in the global scope in case it has been rebound.
///
/// ## Examples
/// ```python
/// dict()
/// dict(a=1, b=2)
/// list()
/// tuple()
/// ```
///
/// Use instead:
/// ```python
/// {}
/// {"a": 1, "b": 2}
/// []
/// ()
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
///
/// ## Options
/// - `lint.flake8-comprehensions.allow-dict-calls-with-keyword-arguments`
#[violation]
pub struct UnnecessaryCollectionCall {
    obj_type: String,
}

impl AlwaysFixableViolation for UnnecessaryCollectionCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryCollectionCall { obj_type } = self;
        format!("Unnecessary `{obj_type}` call (rewrite as a literal)")
    }

    fn fix_title(&self) -> String {
        "Rewrite as a literal".to_string()
    }
}

/// C408
pub(crate) fn unnecessary_collection_call(
    checker: &mut Checker,
    call: &ast::ExprCall,
    settings: &Settings,
) {
    if !call.arguments.args.is_empty() {
        return;
    }
    let Some(builtin) = checker.semantic().resolve_builtin_symbol(&call.func) else {
        return;
    };
    let collection = match builtin {
        "dict"
            if call.arguments.keywords.is_empty()
                || (!settings.allow_dict_calls_with_keyword_arguments
                    && call.arguments.keywords.iter().all(|kw| kw.arg.is_some())) =>
        {
            // `dict()` or `dict(a=1)` (as opposed to `dict(**a)`)
            Collection::Dict
        }
        "list" if call.arguments.keywords.is_empty() => {
            // `list()
            Collection::List
        }
        "tuple" if call.arguments.keywords.is_empty() => {
            // `tuple()`
            Collection::Tuple
        }
        _ => return,
    };

    let mut diagnostic = Diagnostic::new(
        UnnecessaryCollectionCall {
            obj_type: builtin.to_string(),
        },
        call.range(),
    );

    // Convert `dict()` to `{}`.
    if call.arguments.keywords.is_empty() {
        diagnostic.set_fix({
            // Replace from the start of the call to the start of the argument.
            let call_start = Edit::replacement(
                match collection {
                    Collection::Dict => {
                        pad_start("{", call.range(), checker.locator(), checker.semantic())
                    }
                    Collection::List => "[".to_string(),
                    Collection::Tuple => "(".to_string(),
                },
                call.start(),
                call.arguments.start() + TextSize::from(1),
            );

            // Replace from the end of the inner list or tuple to the end of the call with `}`.
            let call_end = Edit::replacement(
                match collection {
                    Collection::Dict => {
                        pad_end("}", call.range(), checker.locator(), checker.semantic())
                    }
                    Collection::List => "]".to_string(),
                    Collection::Tuple => ")".to_string(),
                },
                call.arguments.end() - TextSize::from(1),
                call.end(),
            );

            Fix::unsafe_edits(call_start, [call_end])
        });
    } else {
        // Convert `dict(a=1, b=2)` to `{"a": 1, "b": 2}`.
        diagnostic.try_set_fix(|| {
            fixes::fix_unnecessary_collection_call(call, checker).map(Fix::unsafe_edit)
        });
    }

    checker.diagnostics.push(diagnostic);
}

enum Collection {
    Tuple,
    List,
    Dict,
}
