use ruff_macros::{define_violation, derive_message_formats};

use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Check for use of `exclude` with `ModelForm`.
    ///
    /// ## Why is this bad?
    /// Any new field that is added to the model will be automatically exposed for modification.
    ///
    /// ## Example
    /// ```python
    /// from django.forms import ModelForm
    ///
    /// class PostForm(ModelForm):
    ///     class Meta:
    ///         model = Post
    ///         exclude = ["author"]
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// from django.forms import ModelForm
    ///
    /// class PostForm(ModelForm):
    ///     class Meta:
    ///         model = Post
    ///         fields = ["title", "content"]
    /// ```
    pub struct ExcludeWithModelForm;
);
impl Violation for ExcludeWithModelForm {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use `exclude` with `ModelForm`, use `fields` instead")
    }
}
