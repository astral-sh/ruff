use ruff_python_ast::{AnyNodeRef, Expr, ExprAttribute, ExprCall};

use crate::checkers::ast::Checker;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum DatetimeModuleAntipattern {
    NoTzArgumentPassed,
    NonePassedToTzArgument,
}

/// Check if the "current expression" being visited is followed
/// in the source code by a chain of `.replace()` calls followed by `.astimezone`.
/// The function operates on the assumption that the current expression
/// is a [`datetime.datetime`][datetime] object.
///
/// For example, given the following Python source code:
///
/// ```py
/// import datetime
///
/// datetime.now().replace(hours=4).replace(minutes=46).astimezone()
/// ```
///
/// The last line will produce an AST looking something like this
/// (this is pseudocode approximating our AST):
///
/// ```rs
/// Call {
///     func: Attribute {
///         value: Call {
///             func: Attribute {
///                 value: Call {
///                    func: Attribute {
///                        value: Call {                    // We are visiting this
///                            func: Attribute {            // expression node here
///                                value: Call {            //
///                                    func: Name {         //
///                                        id: "datetime",  //
///                                    },                   //
///                                },                       //
///                                attr: "now"              //
///                            },                           //
///                        },                               //
///                        attr: "replace"
///                    },
///                 },
///                 attr: "replace"
///             },
///         },
///         attr: "astimezone"
///     },
/// }
/// ```
///
/// The node we are visiting as the "current expression" is deeply
/// nested inside many other expressions. As such, in order to check
/// whether the `datetime.now()` call is followed by 0-or-more `.replace()`
/// calls and then an `.astimezone()` call, we must iterate up through the
/// "parent expressions" in the semantic model, checking if they match this
/// AST pattern.
///
/// [datetime]: https://docs.python.org/3/library/datetime.html#datetime-objects
pub(super) fn followed_by_astimezone(checker: &Checker) -> bool {
    let semantic = checker.semantic();
    let mut last = None;

    for (index, expr) in semantic.current_expressions().enumerate() {
        if index == 0 {
            // datetime.now(...).replace(...).astimezone
            // ^^^^^^^^^^^^^^^^^
            continue;
        }

        if index % 2 == 1 {
            // datetime.now(...).replace(...).astimezone
            //                   ^^^^^^^      ^^^^^^^^^^
            let Expr::Attribute(ExprAttribute { attr, .. }) = expr else {
                return false;
            };

            match attr.as_str() {
                "replace" => last = Some(AnyNodeRef::from(expr)),
                "astimezone" => return true,
                _ => return false,
            }
        } else {
            // datetime.now(...).replace(...).astimezone
            //                          ^^^^^
            let Expr::Call(ExprCall { func, .. }) = expr else {
                return false;
            };

            // Without this branch, we would fail to emit a diagnostic on code like this:
            //
            // ```py
            // foo.replace(datetime.now().replace).astimezone()
            //           # ^^^^^^^^^^^^^^  Diagnostic should be emitted here
            //           #                 since the `datetime.now()` call is not followed
            //           #                 by `.astimezone()`
            // ```
            if !last.is_some_and(|it| it.ptr_eq(AnyNodeRef::from(&**func))) {
                return false;
            }
        }
    }

    false
}
