use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use itertools::Itertools;
use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::{Expr, Parameters, TypeParam, TypeParams};
use ruff_python_semantic::analyze::function_type::{self, FunctionType};
use ruff_python_semantic::analyze::visibility::{is_abstract, is_overload};
use ruff_text_size::{Ranged, TextRange};

/// ## What it does
/// Checks for methods that define a custom `TypeVar` for their return type
/// annotation instead of using `Self`.
///
/// ## Why is this bad?
/// While the semantics are often identical, using `Self` is more intuitive
/// and succinct (per [PEP 673]) than a custom `TypeVar`. For example, the
/// use of `Self` will typically allow for the omission of type parameters
/// on the `self` and `cls` arguments.
///
/// This check currently applies to instance methods that return `self`,
/// class methods that return an instance of `cls`, and `__new__` methods.
///
/// ## Example
///
/// ```pyi
/// class Foo:
///     def __new__(cls: type[_S], *args: str, **kwargs: int) -> _S: ...
///     def foo(self: _S, arg: bytes) -> _S: ...
///     @classmethod
///     def bar(cls: type[_S], arg: int) -> _S: ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// from typing import Self
///
/// class Foo:
///     def __new__(cls, *args: str, **kwargs: int) -> Self: ...
///     def foo(self, arg: bytes) -> Self: ...
///     @classmethod
///     def bar(cls, arg: int) -> Self: ...
/// ```
///
/// ## Fix safety
/// The fix is only available in stub files.
/// It will try to remove all usages and declarations of the custom type variable.
/// Pre-[PEP-695]-style declarations will not be removed.
///
/// If a variable's annotation is too complex to handle,
/// the fix will be marked as display only.
/// Otherwise, it will be marked as safe.
///
/// [PEP 673]: https://peps.python.org/pep-0673/#motivation
/// [PEP 695]: https://peps.python.org/pep-0695/
#[derive(ViolationMetadata)]
pub(crate) struct CustomTypeVarReturnType {
    method_name: String,
    in_stub: bool,
}

impl Violation for CustomTypeVarReturnType {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let method_name = &self.method_name;
        format!("Methods like `{method_name}` should return `Self` instead of a custom `TypeVar`")
    }

    fn fix_title(&self) -> Option<String> {
        // See `replace_custom_typevar_with_self`'s doc comment
        if self.in_stub {
            Some("Replace with `Self`".to_string())
        } else {
            None
        }
    }
}

/// PYI019
pub(crate) fn custom_type_var_return_type(
    checker: &mut Checker,
    function_def: &ast::StmtFunctionDef,
) {
    // Given, e.g., `def foo(self: _S, arg: bytes) -> _T`, extract `_T`.
    let Some(returns) = function_def.returns.as_ref() else {
        return;
    };

    let parameters = &*function_def.parameters;

    // Given, e.g., `def foo(self: _S, arg: bytes)`, extract `_S`.
    let Some(self_or_cls_annotation) = parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .next()
        .and_then(|parameter_with_default| parameter_with_default.parameter.annotation.as_ref())
    else {
        return;
    };

    let decorator_list = &*function_def.decorator_list;

    let semantic = checker.semantic();

    // Skip any abstract, static, and overloaded methods.
    if is_abstract(decorator_list, semantic) || is_overload(decorator_list, semantic) {
        return;
    }

    let method = match function_type::classify(
        &function_def.name,
        decorator_list,
        semantic.current_scope(),
        semantic,
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    ) {
        FunctionType::Function => return,
        FunctionType::StaticMethod => return,
        FunctionType::ClassMethod => Method::Class(ClassMethod {
            cls_annotation: self_or_cls_annotation,
            returns,
            type_params: function_def.type_params.as_deref(),
        }),
        FunctionType::Method => Method::Instance(InstanceMethod {
            self_annotation: self_or_cls_annotation,
            returns,
            type_params: function_def.type_params.as_deref(),
        }),
    };

    if method.uses_custom_var() {
        add_diagnostic(checker, function_def, returns);
    }
}

#[derive(Debug)]
enum Method<'a> {
    Class(ClassMethod<'a>),
    Instance(InstanceMethod<'a>),
}

impl Method<'_> {
    fn uses_custom_var(&self) -> bool {
        match self {
            Self::Class(class_method) => class_method.uses_custom_var(),
            Self::Instance(instance_method) => instance_method.uses_custom_var(),
        }
    }
}

#[derive(Debug)]
struct ClassMethod<'a> {
    cls_annotation: &'a Expr,
    returns: &'a Expr,
    type_params: Option<&'a TypeParams>,
}

impl ClassMethod<'_> {
    /// Returns `true` if the class method is annotated with
    /// a custom `TypeVar` that is likely private.
    fn uses_custom_var(&self) -> bool {
        let Expr::Subscript(ast::ExprSubscript { slice, value, .. }) = self.cls_annotation else {
            return false;
        };

        let Expr::Name(value) = value.as_ref() else {
            return false;
        };

        // Don't error if the first argument is annotated with typing.Type[T].
        // These are edge cases, and it's hard to give good error messages for them.
        if value.id != "type" {
            return false;
        };

        let Expr::Name(slice) = slice.as_ref() else {
            return false;
        };

        let Expr::Name(return_annotation) = map_subscript(self.returns) else {
            return false;
        };

        if slice.id != return_annotation.id {
            return false;
        }

        is_likely_private_typevar(&slice.id, self.type_params)
    }
}

#[derive(Debug)]
struct InstanceMethod<'a> {
    self_annotation: &'a Expr,
    returns: &'a Expr,
    type_params: Option<&'a TypeParams>,
}

impl InstanceMethod<'_> {
    /// Returns `true` if the instance method is annotated with
    /// a custom `TypeVar` that is likely private.
    fn uses_custom_var(&self) -> bool {
        let Expr::Name(ast::ExprName {
            id: first_arg_type, ..
        }) = self.self_annotation
        else {
            return false;
        };

        let Expr::Name(ast::ExprName {
            id: return_type, ..
        }) = map_subscript(self.returns)
        else {
            return false;
        };

        if first_arg_type != return_type {
            return false;
        }

        is_likely_private_typevar(first_arg_type, self.type_params)
    }
}

/// Returns `true` if the type variable is likely private.
fn is_likely_private_typevar(type_var_name: &str, type_params: Option<&TypeParams>) -> bool {
    // Ex) `_T`
    if type_var_name.starts_with('_') {
        return true;
    }
    // Ex) `class Foo[T]: ...`
    type_params.is_some_and(|type_params| {
        type_params.iter().any(|type_param| {
            if let TypeParam::TypeVar(ast::TypeParamTypeVar { name, .. }) = type_param {
                name == type_var_name
            } else {
                false
            }
        })
    })
}

fn add_diagnostic(checker: &mut Checker, function_def: &ast::StmtFunctionDef, returns: &Expr) {
    let in_stub = checker.source_type.is_stub();

    let mut diagnostic = Diagnostic::new(
        CustomTypeVarReturnType {
            method_name: function_def.name.to_string(),
            in_stub,
        },
        returns.range(),
    );

    // See `replace_custom_typevar_with_self`'s doc comment
    if in_stub {
        if let Some(fix) = replace_custom_typevar_with_self(checker, function_def, returns) {
            diagnostic.set_fix(fix);
        }
    }

    checker.diagnostics.push(diagnostic);
}

/// Add a "Replace with `Self`" fix that does the following:
///
/// * Import `Self` if necessary
/// * Remove the first parameter's annotation
/// * Replace the return annotation with `Self`
/// * Replace other uses of the original type variable elsewhere in the signature with `Self`
/// * Remove that type variable from the PEP 695 type parameter list
///
/// This fix cannot be suggested for non-stubs,
/// as a non-stub fix would have to deal with references in body/at runtime as well,
/// which is substantially harder and requires a type-aware backend.
///
/// The fourth step above has the same problem.
/// This function thus only does replacements for the simplest of cases
/// and will mark the fix as unsafe if an annotation cannot be handled.
fn replace_custom_typevar_with_self(
    checker: &Checker,
    function_def: &ast::StmtFunctionDef,
    returns: &Expr,
) -> Option<Fix> {
    if checker.settings.preview.is_disabled() {
        return None;
    }

    // The return annotation is guaranteed to be a name,
    // as verified by `uses_custom_var()`.
    let typevar_name = returns.as_name_expr().unwrap().id();

    let mut all_edits = vec![
        replace_return_annotation_with_self(returns),
        remove_first_parameter_annotation(&function_def.parameters),
    ];

    let edit = import_self(checker, returns.range())?;
    all_edits.push(edit);

    if let Some(edit) =
        remove_typevar_declaration(function_def.type_params.as_deref(), typevar_name)
    {
        all_edits.push(edit);
    }

    let (mut edits, fix_applicability) =
        replace_typevar_usages_with_self(&function_def.parameters, typevar_name);
    all_edits.append(&mut edits);

    let (first, rest) = (all_edits.swap_remove(0), all_edits);

    Some(Fix::applicable_edits(first, rest, fix_applicability))
}

fn import_self(checker: &Checker, return_range: TextRange) -> Option<Edit> {
    // From PYI034's fix
    let target_version = checker.settings.target_version.as_tuple();
    let source_module = if target_version >= (3, 11) {
        "typing"
    } else {
        "typing_extensions"
    };

    let (importer, semantic) = (checker.importer(), checker.semantic());
    let request = ImportRequest::import_from(source_module, "Self");

    let position = return_range.start();
    let (edit, ..) = importer
        .get_or_import_symbol(&request, position, semantic)
        .ok()?;

    Some(edit)
}

fn remove_first_parameter_annotation(parameters: &Parameters) -> Edit {
    // The first parameter is guaranteed to be `self`/`cls`,
    // as verified by `uses_custom_var()`.
    let mut non_variadic_positional = parameters.posonlyargs.iter().chain(&parameters.args);
    let first = &non_variadic_positional.next().unwrap().parameter;

    let name_end = first.name.range.end();
    let annotation_end = first.range.end();

    Edit::deletion(name_end, annotation_end)
}

fn replace_return_annotation_with_self(returns: &Expr) -> Edit {
    Edit::range_replacement("Self".to_string(), returns.range())
}

fn replace_typevar_usages_with_self(
    parameters: &Parameters,
    typevar_name: &str,
) -> (Vec<Edit>, Applicability) {
    let mut edits = vec![];
    let mut could_not_handle_all_usages = false;

    for parameter in parameters.iter().skip(1) {
        let Some(annotation) = parameter.annotation() else {
            continue;
        };
        let Expr::Name(name) = annotation else {
            could_not_handle_all_usages = true;
            continue;
        };

        if name.id.as_str() == typevar_name {
            let edit = Edit::range_replacement("Self".to_string(), annotation.range());
            edits.push(edit);
        } else {
            could_not_handle_all_usages = true;
        }
    }

    if could_not_handle_all_usages {
        (edits, Applicability::DisplayOnly)
    } else {
        (edits, Applicability::Safe)
    }
}

fn remove_typevar_declaration(type_params: Option<&TypeParams>, name: &str) -> Option<Edit> {
    let is_declaration_in_question = |type_param: &&TypeParam| -> bool {
        if let TypeParam::TypeVar(typevar) = type_param {
            return typevar.name.as_str() == name;
        };

        false
    };

    let parameter_list = type_params?;
    let parameters = &parameter_list.type_params;
    let first = parameters.first()?;

    if parameter_list.len() == 1 && is_declaration_in_question(&first) {
        return Some(Edit::range_deletion(parameter_list.range));
    }

    let (index, declaration) = parameters
        .iter()
        .find_position(is_declaration_in_question)?;

    let typevar_range = declaration.range();
    let last_index = parameters.len() - 1;

    let range = if index < last_index {
        // [A, B, C]
        //     ^^^ Remove this
        let next_range = parameters[index + 1].range();
        TextRange::new(typevar_range.start(), next_range.start())
    } else {
        // [A, B, C]
        //      ^^^ Remove this
        let previous_range = parameters[index - 1].range();
        TextRange::new(previous_range.end(), typevar_range.start())
    };

    Some(Edit::range_deletion(range))
}
