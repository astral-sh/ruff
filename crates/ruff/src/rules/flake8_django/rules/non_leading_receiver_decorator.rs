use ruff_python_ast::Decorator;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that Django's `@receiver` decorator is listed first, prior to
/// any other decorators.
///
/// ## Why is this bad?
/// Django's `@receiver` decorator is special in that it does not return
/// a wrapped function. Rather, `@receiver` connects the decorated function
/// to a signal. If any other decorators are listed before `@receiver`,
/// the decorated function will not be connected to the signal.
///
/// ## Example
/// ```python
/// from django.dispatch import receiver
/// from django.db.models.signals import post_save
///
///
/// @transaction.atomic
/// @receiver(post_save, sender=MyModel)
/// def my_handler(sender, instance, created, **kwargs):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// from django.dispatch import receiver
/// from django.db.models.signals import post_save
///
///
/// @receiver(post_save, sender=MyModel)
/// @transaction.atomic
/// def my_handler(sender, instance, created, **kwargs):
///     pass
/// ```
#[violation]
pub struct DjangoNonLeadingReceiverDecorator;

impl Violation for DjangoNonLeadingReceiverDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`@receiver` decorator must be on top of all the other decorators")
    }
}

/// DJ013
pub(crate) fn non_leading_receiver_decorator(checker: &mut Checker, decorator_list: &[Decorator]) {
    let mut seen_receiver = false;
    for (i, decorator) in decorator_list.iter().enumerate() {
        let is_receiver = decorator.expression.as_call_expr().is_some_and(|call| {
            checker
                .semantic()
                .resolve_call_path(&call.func)
                .is_some_and(|call_path| {
                    matches!(call_path.as_slice(), ["django", "dispatch", "receiver"])
                })
        });
        if i > 0 && is_receiver && !seen_receiver {
            checker.diagnostics.push(Diagnostic::new(
                DjangoNonLeadingReceiverDecorator,
                decorator.range(),
            ));
        }
        if !is_receiver && seen_receiver {
            seen_receiver = false;
        } else if is_receiver {
            seen_receiver = true;
        }
    }
}
