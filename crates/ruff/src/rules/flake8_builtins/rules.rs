use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::builtins::BUILTINS;
use rustpython_parser::ast::Located;

use super::types::ShadowingType;
use crate::ast::types::Range;
use crate::registry::{Diagnostic, DiagnosticKind};
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Checks for any variable (and function) assignments that have the same
    /// name as a builtin.
    ///
    /// Keep in mind that this also takes into account the [`builtins`] and
    /// [`flake8-builtins.builtins-ignorelist`] configuration options.
    ///
    /// ## Why is this bad?
    /// Using a builtin name as the name of a variable increases
    /// the difficulty of reading and maintaining the code, can cause
    /// non-obvious code errors, and can mess up code highlighters.
    ///
    /// Instead, the variable should be renamed to something else
    /// that is not considered a builtin. If you are sure that you want
    /// to name the variable this way, you can also edit the [`flake8-builtins.builtins-ignorelist`]
    /// configuration option.
    ///
    /// ## Options
    ///
    /// * `builtins`
    /// * `flake8-builtins.builtins-ignorelist`
    ///
    /// ## Example
    /// ```python
    /// def find_max(list_of_lists):
    ///     max = 0
    ///     for flat_list in list_of_lists:
    ///         for value in flat_list:
    ///             # This is confusing, and causes an error!
    ///             max = max(max, value)  # TypeError: 'int' object is not callable
    ///     return max
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// def find_max(list_of_lists):
    ///     result = 0
    ///     for flat_list in list_of_lists:
    ///         for value in flat_list:
    ///             result = max(result, value)
    ///     return result
    /// ```
    ///
    /// * [StackOverflow - Why is it a bad idea to name a variable `id` in Python?](https://stackoverflow.com/questions/77552/id-is-a-bad-variable-name-in-python)
    pub struct BuiltinVariableShadowing {
        pub name: String,
    }
);
impl Violation for BuiltinVariableShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinVariableShadowing { name } = self;
        format!("Variable `{name}` is shadowing a python builtin")
    }
}

define_violation!(
    /// ## What it does
    /// Checks for any function arguments that have the same name as a builtin.
    ///
    /// Keep in mind that this also takes into account the [`builtins`] and
    /// [`flake8-builtins.builtins-ignorelist`] configuration options.
    ///
    /// ## Why is this bad?
    /// Using a builtin name as the name of an argument name increases
    /// the difficulty of reading and maintaining the code, can cause
    /// non-obvious code errors, and can mess up code highlighters.
    ///
    /// Instead, the function argument should be renamed to something else
    /// that is not considered a builtin. If you are sure that you want
    /// to name the argument this way, you can also edit the [`flake8-builtins.builtins-ignorelist`]
    /// configuration option.
    ///
    /// ## Options
    ///
    /// * `builtins`
    /// * `flake8-builtins.builtins-ignorelist`
    ///
    /// ## Example
    /// ```python
    /// def remove_duplicates(list, list2):
    ///     result = set()
    ///     for value in list:
    ///         result.add(value)
    ///     for value in list2:
    ///         result.add(value)
    ///     return list(result)  # TypeError: 'list' object is not callable
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// def remove_duplicates(list1, list2):
    ///     result = set()
    ///     for value in list1:
    ///         result.add(value)
    ///     for value in list2:
    ///         result.add(value)
    ///     return list(result)  
    /// ```
    ///
    /// ## References
    /// - [StackOverflow - Is it bad practice to use a built-in function name as an attribute or method identifier?](https://stackoverflow.com/questions/9109333/is-it-bad-practice-to-use-a-built-in-function-name-as-an-attribute-or-method-ide)
    /// - [StackOverflow - Why is it a bad idea to name a variable `id` in Python?](https://stackoverflow.com/questions/77552/id-is-a-bad-variable-name-in-python)
    pub struct BuiltinArgumentShadowing {
        pub name: String,
    }
);
impl Violation for BuiltinArgumentShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinArgumentShadowing { name } = self;
        format!("Argument `{name}` is shadowing a python builtin")
    }
}

define_violation!(
    /// ## What it does
    /// Checks for any class attributes that have the same name as a builtin.
    ///
    /// Keep in mind that this also takes into account the [`builtins`] and
    /// [`flake8-builtins.builtins-ignorelist`] configuration options.
    ///
    /// ## Why is this bad?
    /// Using a builtin name as the name of a class attribute increases
    /// the difficulty of reading and maintaining the code, can cause
    /// non-obvious code errors, and can mess up code highlighters.
    ///
    /// Instead, the attribute should be renamed to something else
    /// that is not considered a builtin or converted to the related dunder
    /// (aka magic) method.If you are sure that you want to name the attribute
    /// this way, you can also edit the [`flake8-builtins.builtins-ignorelist`] configuration option.
    ///
    /// ## Options
    ///
    /// * `builtins`
    /// * `flake8-builtins.builtins-ignorelist`
    ///
    /// ## Example
    /// ```python
    /// class Shadow:
    ///     def int():
    ///         return 0
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// class Shadow:
    ///     def to_int():
    ///         return 0
    ///     # OR (keep in mind you will have to use `int(shadow)` instead of `shadow.int()`)
    ///     def __int__():
    ///         return 0
    /// ```
    ///
    /// ## References
    /// - [StackOverflow - Is it bad practice to use a built-in function name as an attribute or method identifier?](https://stackoverflow.com/questions/9109333/is-it-bad-practice-to-use-a-built-in-function-name-as-an-attribute-or-method-ide)
    /// - [StackOverflow - Why is it a bad idea to name a variable `id` in Python?](https://stackoverflow.com/questions/77552/id-is-a-bad-variable-name-in-python)
    pub struct BuiltinAttributeShadowing {
        pub name: String,
    }
);
impl Violation for BuiltinAttributeShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinAttributeShadowing { name } = self;
        format!("Class attribute `{name}` is shadowing a python builtin")
    }
}

/// Check builtin name shadowing.
pub fn builtin_shadowing<T>(
    name: &str,
    located: &Located<T>,
    node_type: ShadowingType,
    ignorelist: &[String],
) -> Option<Diagnostic> {
    if BUILTINS.contains(&name) && !ignorelist.contains(&name.to_string()) {
        Some(Diagnostic::new::<DiagnosticKind>(
            match node_type {
                ShadowingType::Variable => BuiltinVariableShadowing {
                    name: name.to_string(),
                }
                .into(),
                ShadowingType::Argument => BuiltinArgumentShadowing {
                    name: name.to_string(),
                }
                .into(),
                ShadowingType::Attribute => BuiltinAttributeShadowing {
                    name: name.to_string(),
                }
                .into(),
            },
            Range::from_located(located),
        ))
    } else {
        None
    }
}
