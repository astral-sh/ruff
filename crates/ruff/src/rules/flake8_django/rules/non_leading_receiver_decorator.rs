use rustpython_parser::ast::{Expr, ExprKind};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::{CallPath, Range};
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
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
    /// @transaction.atomic
    /// @receiver(post_save, sender=MyModel)
    /// def my_handler(sender, instance, created, **kwargs):
    ///    pass
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// from django.dispatch import receiver
    /// from django.db.models.signals import post_save
    ///
    /// @receiver(post_save, sender=MyModel)
    /// @transaction.atomic
    /// def my_handler(sender, instance, created, **kwargs):
    ///     pass
    /// ```
    pub struct NonLeadingReceiverDecorator;
);
impl Violation for NonLeadingReceiverDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`@receiver` decorator must be on top of all the other decorators")
    }
}

/// DJ013
pub fn non_leading_receiver_decorator<'a, F>(
    decorator_list: &'a [Expr],
    resolve_call_path: F,
) -> Vec<Diagnostic>
where
    F: Fn(&'a Expr) -> Option<CallPath<'a>>,
{
    let mut diagnostics = vec![];
    let mut seen_receiver = false;
    for (i, decorator) in decorator_list.iter().enumerate() {
        let is_receiver = match &decorator.node {
            ExprKind::Call { func, .. } => resolve_call_path(func).map_or(false, |call_path| {
                call_path.as_slice() == ["django", "dispatch", "receiver"]
            }),
            _ => false,
        };
        if i > 0 && is_receiver && !seen_receiver {
            diagnostics.push(Diagnostic::new(
                NonLeadingReceiverDecorator,
                Range::from_located(decorator),
            ));
        }
        if !is_receiver && seen_receiver {
            seen_receiver = false;
        } else if is_receiver {
            seen_receiver = true;
        }
    }
    diagnostics
}
