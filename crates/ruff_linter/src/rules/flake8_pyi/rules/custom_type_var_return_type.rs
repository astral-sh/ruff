use itertools::Itertools;
use std::cmp;

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_semantic::analyze::function_type::{self, FunctionType};
use ruff_python_semantic::analyze::visibility::{is_abstract, is_overload};
use ruff_python_semantic::{Binding, ScopeId, SemanticModel};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::importer::{ImportRequest, ResolutionError};
use crate::settings::types::PythonVersion;

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
/// If there are any comments within the fix ranges, it will be marked as unsafe.
/// Otherwise, it will be marked as safe.
///
/// [PEP 673]: https://peps.python.org/pep-0673/#motivation
/// [PEP 695]: https://peps.python.org/pep-0695/
#[derive(ViolationMetadata)]
pub(crate) struct CustomTypeVarReturnType {
    method_name: String,
}

impl Violation for CustomTypeVarReturnType {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let method_name = &self.method_name;
        format!("Methods like `{method_name}` should return `Self` instead of a custom `TypeVar`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Self`".to_string())
    }
}

/// PYI019
pub(crate) fn custom_type_var_return_type(
    checker: &Checker,
    binding: &Binding,
) -> Option<Diagnostic> {
    let semantic = checker.semantic();
    let current_scope = &semantic.scopes[binding.scope];
    let function_def = binding.statement(semantic)?.as_function_def_stmt()?;

    let ast::StmtFunctionDef {
        name: function_name,
        parameters,
        returns,
        decorator_list,
        type_params,
        ..
    } = function_def;

    let returns = returns.as_deref()?;

    // Given, e.g., `def foo(self: _S, arg: bytes)`, extract `_S`.
    let self_or_cls_parameter = parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .next()?;

    let self_or_cls_annotation = self_or_cls_parameter.annotation()?;

    // Skip any abstract, static, and overloaded methods.
    if is_abstract(decorator_list, semantic) || is_overload(decorator_list, semantic) {
        return None;
    }

    let method = match function_type::classify(
        function_name,
        decorator_list,
        current_scope,
        semantic,
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    ) {
        FunctionType::Function => return None,
        FunctionType::StaticMethod => return None,
        FunctionType::ClassMethod => Method::Class(ClassMethod {
            cls_annotation: self_or_cls_annotation,
            returns,
            type_params: type_params.as_deref(),
        }),
        FunctionType::Method => Method::Instance(InstanceMethod {
            self_annotation: self_or_cls_annotation,
            returns,
            type_params: type_params.as_deref(),
        }),
    };

    if !method.uses_custom_var(semantic, binding.scope) {
        return None;
    }

    let mut diagnostic = Diagnostic::new(
        CustomTypeVarReturnType {
            method_name: function_name.to_string(),
        },
        returns.range(),
    );

    diagnostic.try_set_optional_fix(|| {
        replace_custom_typevar_with_self(
            checker,
            function_def,
            self_or_cls_parameter,
            self_or_cls_annotation,
            returns,
        )
    });

    Some(diagnostic)
}

#[derive(Debug)]
enum Method<'a> {
    Class(ClassMethod<'a>),
    Instance(InstanceMethod<'a>),
}

impl Method<'_> {
    fn uses_custom_var(&self, semantic: &SemanticModel, scope: ScopeId) -> bool {
        match self {
            Self::Class(class_method) => class_method.uses_custom_var(semantic, scope),
            Self::Instance(instance_method) => instance_method.uses_custom_var(),
        }
    }
}

#[derive(Debug)]
struct ClassMethod<'a> {
    cls_annotation: &'a ast::Expr,
    returns: &'a ast::Expr,
    type_params: Option<&'a ast::TypeParams>,
}

impl ClassMethod<'_> {
    /// Returns `true` if the class method is annotated with
    /// a custom `TypeVar` that is likely private.
    fn uses_custom_var(&self, semantic: &SemanticModel, scope: ScopeId) -> bool {
        let ast::Expr::Subscript(ast::ExprSubscript {
            value: cls_annotation_value,
            slice: cls_annotation_typevar,
            ..
        }) = self.cls_annotation
        else {
            return false;
        };

        let ast::Expr::Name(cls_annotation_typevar) = &**cls_annotation_typevar else {
            return false;
        };

        let cls_annotation_typevar = &cls_annotation_typevar.id;

        let ast::Expr::Name(ast::ExprName { id, .. }) = &**cls_annotation_value else {
            return false;
        };

        if id != "type" {
            return false;
        }

        if !semantic.has_builtin_binding_in_scope("type", scope) {
            return false;
        }

        let return_annotation_typevar = match self.returns {
            ast::Expr::Name(ast::ExprName { id, .. }) => id,
            ast::Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                let ast::Expr::Name(return_annotation_typevar) = &**slice else {
                    return false;
                };
                let ast::Expr::Name(ast::ExprName { id, .. }) = &**value else {
                    return false;
                };
                if id != "type" {
                    return false;
                }
                &return_annotation_typevar.id
            }
            _ => return false,
        };

        if cls_annotation_typevar != return_annotation_typevar {
            return false;
        }

        is_likely_private_typevar(cls_annotation_typevar, self.type_params)
    }
}

#[derive(Debug)]
struct InstanceMethod<'a> {
    self_annotation: &'a ast::Expr,
    returns: &'a ast::Expr,
    type_params: Option<&'a ast::TypeParams>,
}

impl InstanceMethod<'_> {
    /// Returns `true` if the instance method is annotated with
    /// a custom `TypeVar` that is likely private.
    fn uses_custom_var(&self) -> bool {
        let ast::Expr::Name(ast::ExprName {
            id: first_arg_type, ..
        }) = self.self_annotation
        else {
            return false;
        };

        let ast::Expr::Name(ast::ExprName {
            id: return_type, ..
        }) = self.returns
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
fn is_likely_private_typevar(type_var_name: &str, type_params: Option<&ast::TypeParams>) -> bool {
    // Ex) `_T`
    if type_var_name.starts_with('_') {
        return true;
    }
    // Ex) `class Foo[T]: ...`
    type_params.is_some_and(|type_params| {
        type_params.iter().any(|type_param| {
            if let ast::TypeParam::TypeVar(ast::TypeParamTypeVar { name, .. }) = type_param {
                name == type_var_name
            } else {
                false
            }
        })
    })
}

/// Add a "Replace with `Self`" fix that does the following:
///
/// * Import `Self` if necessary
/// * Remove the first parameter's annotation
/// * Replace the return annotation with `Self`
/// * Replace other uses of the original type variable elsewhere in the signature with `Self`
/// * Remove that type variable from the PEP 695 type parameter list
fn replace_custom_typevar_with_self(
    checker: &Checker,
    function_def: &ast::StmtFunctionDef,
    self_or_cls_parameter: &ast::ParameterWithDefault,
    self_or_cls_annotation: &ast::Expr,
    returns: &ast::Expr,
) -> anyhow::Result<Option<Fix>> {
    if checker.settings.preview.is_disabled() {
        return Ok(None);
    }

    // This fix cannot be suggested for non-stubs,
    // as a non-stub fix would have to deal with references in body/at runtime as well,
    // which is substantially harder and requires a type-aware backend.
    if !checker.source_type.is_stub() {
        return Ok(None);
    }

    // Non-`Name` return annotations are not currently autofixed
    let ast::Expr::Name(typevar) = &returns else {
        return Ok(None);
    };

    let mut applicability = Applicability::Safe;

    let typevar_name = &typevar.id;

    let (import_edit, self_symbol_binding) = import_self(checker, returns.start())?;

    let mut other_edits = vec![Edit::deletion(
        self_or_cls_parameter.name().end(),
        self_or_cls_annotation.end(),
    )];

    let replace_references_range = TextRange::new(self_or_cls_annotation.end(), returns.end());

    other_edits.extend(remove_typevar_declaration(
        function_def.type_params.as_deref(),
        typevar_name,
    ));

    if let Some(edits) = replace_typevar_usages_with_self(
        typevar,
        &self_symbol_binding,
        replace_references_range,
        checker.semantic(),
    ) {
        other_edits.extend(edits);
    } else {
        applicability = Applicability::DisplayOnly;
    }

    let comment_ranges = checker.comment_ranges();

    if other_edits
        .iter()
        .any(|edit| comment_ranges.intersects(edit.range()))
    {
        applicability = cmp::min(applicability, Applicability::Unsafe);
    }

    Ok(Some(Fix::applicable_edits(
        import_edit,
        other_edits,
        applicability,
    )))
}

fn import_self(checker: &Checker, position: TextSize) -> Result<(Edit, String), ResolutionError> {
    // See also PYI034's fix
    let source_module = if checker.settings.target_version >= PythonVersion::Py311 {
        "typing"
    } else {
        "typing_extensions"
    };
    let request = ImportRequest::import_from(source_module, "Self");
    checker
        .importer()
        .get_or_import_symbol(&request, position, checker.semantic())
}

/// Returns a series of [`Edit`]s that modify all references to the given `typevar`,
/// or `None` when it is not possible to resolve the binding.
///
/// Only references within `editable_range` will be modified.
/// This ensures that no edit in this series will overlap with other edits.
fn replace_typevar_usages_with_self(
    typevar: &ast::ExprName,
    self_symbol_binding: &str,
    editable_range: TextRange,
    semantic: &SemanticModel,
) -> Option<Vec<Edit>> {
    let binding = semantic
        .only_binding(typevar)
        .map(|id| semantic.binding(id))?;

    let mut edits = vec![];

    for reference_id in binding.references() {
        let reference = semantic.reference(reference_id);
        let range = reference.range();

        if editable_range.contains_range(range) {
            let edit = Edit::range_replacement(self_symbol_binding.to_string(), range);
            edits.push(edit);
        }
    }

    Some(edits)
}

fn remove_typevar_declaration(type_params: Option<&ast::TypeParams>, name: &str) -> Option<Edit> {
    let is_declaration_in_question = |type_param: &&ast::TypeParam| -> bool {
        if let ast::TypeParam::TypeVar(typevar) = type_param {
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

    let last_index = parameters.len() - 1;

    let range = if index < last_index {
        // [A, B, C]
        //     ^^^ Remove this
        TextRange::new(declaration.start(), parameters[index + 1].start())
    } else {
        // [A, B, C]
        //      ^^^ Remove this
        TextRange::new(parameters[index - 1].end(), declaration.end())
    };

    Some(Edit::range_deletion(range))
}
