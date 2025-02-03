use anyhow::{bail, Context};

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_semantic::analyze::function_type::{self, FunctionType};
use ruff_python_semantic::analyze::visibility::{is_abstract, is_overload};
use ruff_python_semantic::{Binding, ResolvedReference, ScopeId, SemanticModel};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::importer::{ImportRequest, ResolutionError};
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks for methods that use custom `TypeVar`s in their annotations
/// when they could use `Self` instead.
///
/// ## Why is this bad?
/// While the semantics are often identical, using `Self` is more intuitive
/// and succinct (per [PEP 673]) than a custom `TypeVar`. For example, the
/// use of `Self` will typically allow for the omission of type parameters
/// on the `self` and `cls` arguments.
///
/// This check currently applies to instance methods that return `self`,
/// class methods that return an instance of `cls`, class methods that return
/// `cls`, and `__new__` methods.
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
/// ## Preview-mode behaviour
/// This rule's behaviour has several differences when [`preview`] mode is enabled:
/// 1. The fix for this rule is currently only available if `preview` mode is enabled.
/// 2. By default, this rule is only applied to methods that have return-type annotations,
///    and the range of the diagnostic is the range of the return-type annotation.
///    In preview mode, this rule is also applied to some methods that do not have
///    return-type annotations. The range of the diagnostic is the range of the function
///    header (from the end of the function name to the end of the parameters).
/// 3. In `preview` mode, the rule uses different logic to determine whether an annotation
///    refers to a type variable. The `preview`-mode logic is more accurate, but may lead
///    to more methods being flagged than if `preview` mode is disabled.
///
/// [PEP 673]: https://peps.python.org/pep-0673/#motivation
/// [PEP 695]: https://peps.python.org/pep-0695/
#[derive(ViolationMetadata)]
pub(crate) struct CustomTypeVarForSelf {
    typevar_name: String,
}

impl Violation for CustomTypeVarForSelf {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use `Self` instead of custom TypeVar `{}`",
            &self.typevar_name
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!(
            "Replace TypeVar `{}` with `Self`",
            &self.typevar_name
        ))
    }
}

/// PYI019
pub(crate) fn custom_type_var_instead_of_self(
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

    let type_params = type_params.as_deref();

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

    let function_kind = function_type::classify(
        function_name,
        decorator_list,
        current_scope,
        semantic,
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    );

    let function_header_end = returns
        .as_deref()
        .map(Ranged::end)
        .unwrap_or_else(|| parameters.end());

    // In stable mode, we only emit the diagnostic on methods that have a return type annotation.
    // In preview mode, we have a more principled approach to determine if an annotation refers
    // to a type variable, and we emit the diagnostic on some methods that do not have return
    // annotations.
    let (method, diagnostic_range) = match function_kind {
        FunctionType::ClassMethod => {
            if checker.settings.preview.is_enabled() {
                (
                    Method::PreviewClass(PreviewClassMethod {
                        cls_annotation: self_or_cls_annotation,
                        type_params,
                    }),
                    TextRange::new(function_name.end(), function_header_end),
                )
            } else {
                returns.as_deref().map(|returns| {
                    (
                        Method::Class(ClassMethod {
                            cls_annotation: self_or_cls_annotation,
                            returns,
                            type_params,
                        }),
                        returns.range(),
                    )
                })?
            }
        }
        FunctionType::Method => {
            if checker.settings.preview.is_enabled() {
                (
                    Method::PreviewInstance(PreviewInstanceMethod {
                        self_annotation: self_or_cls_annotation,
                        type_params,
                    }),
                    TextRange::new(function_name.end(), function_header_end),
                )
            } else {
                returns.as_deref().map(|returns| {
                    (
                        Method::Instance(InstanceMethod {
                            self_annotation: self_or_cls_annotation,
                            returns,
                            type_params,
                        }),
                        returns.range(),
                    )
                })?
            }
        }
        FunctionType::Function | FunctionType::StaticMethod => return None,
    };

    let custom_typevar = method.custom_typevar(semantic, binding.scope)?;

    let mut diagnostic = Diagnostic::new(
        CustomTypeVarForSelf {
            typevar_name: custom_typevar.name.to_string(),
        },
        diagnostic_range,
    );

    diagnostic.try_set_optional_fix(|| {
        replace_custom_typevar_with_self(
            checker,
            function_def,
            &custom_typevar,
            self_or_cls_parameter,
            self_or_cls_annotation,
            function_header_end,
        )
    });

    Some(diagnostic)
}

#[derive(Debug)]
enum Method<'a> {
    Class(ClassMethod<'a>),
    PreviewClass(PreviewClassMethod<'a>),
    Instance(InstanceMethod<'a>),
    PreviewInstance(PreviewInstanceMethod<'a>),
}

impl Method<'_> {
    fn custom_typevar<'a>(
        &'a self,
        semantic: &'a SemanticModel<'a>,
        scope: ScopeId,
    ) -> Option<TypeVar<'a>> {
        match self {
            Self::Class(class_method) => class_method.custom_typevar(semantic, scope),
            Self::PreviewClass(class_method) => class_method.custom_typevar(semantic, scope),
            Self::Instance(instance_method) => instance_method.custom_typevar(semantic),
            Self::PreviewInstance(instance_method) => instance_method.custom_typevar(semantic),
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
    /// Returns `Some(typevar)` if the class method is annotated with
    /// a custom `TypeVar` that is likely private.
    fn custom_typevar<'a>(
        &'a self,
        semantic: &'a SemanticModel<'a>,
        scope: ScopeId,
    ) -> Option<TypeVar<'a>> {
        let ast::ExprSubscript {
            value: cls_annotation_value,
            slice: cls_annotation_typevar,
            ..
        } = self.cls_annotation.as_subscript_expr()?;

        let cls_annotation_typevar = cls_annotation_typevar.as_name_expr()?;
        let cls_annotation_typevar_name = &cls_annotation_typevar.id;
        let ast::ExprName { id, .. } = cls_annotation_value.as_name_expr()?;

        if id != "type" {
            return None;
        }

        if !semantic.has_builtin_binding_in_scope("type", scope) {
            return None;
        }

        let return_annotation_typevar = match self.returns {
            ast::Expr::Name(ast::ExprName { id, .. }) => id,
            ast::Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                let return_annotation_typevar = slice.as_name_expr()?;
                let ast::ExprName { id, .. } = value.as_name_expr()?;
                if id != "type" {
                    return None;
                }
                &return_annotation_typevar.id
            }
            _ => return None,
        };

        if cls_annotation_typevar_name != return_annotation_typevar {
            return None;
        }

        if !is_likely_private_typevar(cls_annotation_typevar_name, self.type_params) {
            return None;
        }

        let binding = semantic
            .resolve_name(cls_annotation_typevar)
            .map(|binding_id| semantic.binding(binding_id))?;

        Some(TypeVar {
            name: cls_annotation_typevar_name,
            binding,
        })
    }
}

/// Struct for implementing this rule as applied to classmethods in preview mode.
///
/// In stable mode, we only emit this diagnostic on methods that have return annotations,
/// so the stable-mode version of this struct has a `returns: &ast::Expr` field. In preview
/// mode, we also emit this diagnostic on methods that do not have return annotations, so
/// the preview-mode version of this struct does not have a `returns` field.
#[derive(Debug)]
struct PreviewClassMethod<'a> {
    cls_annotation: &'a ast::Expr,
    type_params: Option<&'a ast::TypeParams>,
}

impl PreviewClassMethod<'_> {
    /// Returns `Some(typevar)` if the class method is annotated with
    /// a custom `TypeVar` for the `cls` parameter
    fn custom_typevar<'a>(
        &'a self,
        semantic: &'a SemanticModel<'a>,
        scope: ScopeId,
    ) -> Option<TypeVar<'a>> {
        let ast::ExprSubscript {
            value: cls_annotation_value,
            slice: cls_annotation_typevar,
            ..
        } = self.cls_annotation.as_subscript_expr()?;

        let cls_annotation_typevar = cls_annotation_typevar.as_name_expr()?;

        let ast::ExprName { id, .. } = cls_annotation_value.as_name_expr()?;
        if id != "type" {
            return None;
        }
        if !semantic.has_builtin_binding_in_scope("type", scope) {
            return None;
        }

        custom_typevar_preview(cls_annotation_typevar, self.type_params, semantic)
    }
}

#[derive(Debug)]
struct InstanceMethod<'a> {
    self_annotation: &'a ast::Expr,
    returns: &'a ast::Expr,
    type_params: Option<&'a ast::TypeParams>,
}

impl InstanceMethod<'_> {
    /// Returns `Some(typevar)` if the instance method is annotated with
    /// a custom `TypeVar` that is likely private.
    fn custom_typevar<'a>(&'a self, semantic: &'a SemanticModel<'a>) -> Option<TypeVar<'a>> {
        let self_annotation = self.self_annotation.as_name_expr()?;
        let first_arg_type = &self_annotation.id;

        let ast::ExprName {
            id: return_type, ..
        } = self.returns.as_name_expr()?;

        if first_arg_type != return_type {
            return None;
        }

        if !is_likely_private_typevar(first_arg_type, self.type_params) {
            return None;
        }

        let binding = semantic
            .resolve_name(self_annotation)
            .map(|binding_id| semantic.binding(binding_id))?;

        Some(TypeVar {
            name: first_arg_type,
            binding,
        })
    }
}

/// Struct for implementing this rule as applied to instance methods in preview mode.
///
/// In stable mode, we only emit this diagnostic on methods that have return annotations,
/// so the stable-mode version of this struct has a `returns: &ast::Expr` field. In preview
/// mode, we also emit this diagnostic on methods that do not have return annotations, so
/// the preview-mode version of this struct does not have a `returns` field.
#[derive(Debug)]
struct PreviewInstanceMethod<'a> {
    self_annotation: &'a ast::Expr,
    type_params: Option<&'a ast::TypeParams>,
}

impl PreviewInstanceMethod<'_> {
    /// Returns `Some(typevar)` if the instance method is annotated with
    /// a custom `TypeVar` for the `self` parameter
    fn custom_typevar<'a>(&'a self, semantic: &'a SemanticModel<'a>) -> Option<TypeVar<'a>> {
        custom_typevar_preview(
            self.self_annotation.as_name_expr()?,
            self.type_params,
            semantic,
        )
    }
}

/// Returns `true` if the type variable is likely private.
///
/// This routine is only used if `--preview` is not enabled,
/// as it uses heuristics to determine if an annotation uses a type variable.
/// In preview mode, we apply a more principled approach.
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

/// Returns `Some(TypeVar)` if `typevar_expr` refers to a `TypeVar` binding
fn custom_typevar_preview<'a>(
    typevar_expr: &'a ast::ExprName,
    type_params: Option<&ast::TypeParams>,
    semantic: &'a SemanticModel<'a>,
) -> Option<TypeVar<'a>> {
    let binding = semantic
        .resolve_name(typevar_expr)
        .map(|binding_id| semantic.binding(binding_id))?;

    // Example:
    // ```py
    // class Foo:
    //     def m[S](self: S) -> S: ...
    // ```
    if binding.kind.is_type_param() {
        return type_params?
            .iter()
            .filter_map(ast::TypeParam::as_type_var)
            .any(|ast::TypeParamTypeVar { name, .. }| name.id == typevar_expr.id)
            .then_some(TypeVar {
                name: &typevar_expr.id,
                binding,
            });
    }

    // Example:
    // ```py
    // from typing import TypeVar
    //
    // S = TypeVar("S", bound="Foo")
    //
    // class Foo:
    //     def m(self: S) -> S: ...
    // ```
    if !semantic.seen_typing() {
        return None;
    }
    let statement = binding.source.map(|node_id| semantic.statement(node_id))?;
    let rhs_function = statement.as_assign_stmt()?.value.as_call_expr()?;

    semantic
        .match_typing_expr(&rhs_function.func, "TypeVar")
        .then_some(TypeVar {
            name: &typevar_expr.id,
            binding,
        })
}

/// Add a "Replace with `Self`" fix that does the following:
///
/// * Import `Self` if necessary
/// * Remove the first parameter's annotation
/// * Replace other uses of the original type variable elsewhere in the signature with `Self`
/// * If it was a PEP-695 type variable, removes that `TypeVar` from the PEP-695 type-parameter list
fn replace_custom_typevar_with_self(
    checker: &Checker,
    function_def: &ast::StmtFunctionDef,
    custom_typevar: &TypeVar,
    self_or_cls_parameter: &ast::ParameterWithDefault,
    self_or_cls_annotation: &ast::Expr,
    function_header_end: TextSize,
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

    // (1) Import `Self` (if necessary)
    let (import_edit, self_symbol_binding) = import_self(checker, function_def.start())?;

    // (2) Remove the first parameter's annotation
    let mut other_edits = vec![Edit::deletion(
        self_or_cls_parameter.name().end(),
        self_or_cls_annotation.end(),
    )];

    // (3) If it was a PEP-695 type variable, remove that `TypeVar` from the PEP-695 type-parameter list
    if custom_typevar.is_pep695_typevar() {
        let Some(type_params) = function_def.type_params.as_deref() else {
            bail!("Should not be possible to have a type parameter without a type parameter list");
        };
        let deletion_edit = remove_pep695_typevar_declaration(type_params, custom_typevar)
            .context("Failed to find a `TypeVar` in the type params that matches the binding")?;
        other_edits.push(deletion_edit);
    }

    // (4) Replace all other references to the original type variable elsewhere in the function's header
    //     with `Self`
    let replace_references_range =
        TextRange::new(self_or_cls_annotation.end(), function_header_end);

    other_edits.extend(replace_typevar_usages_with_self(
        custom_typevar,
        self_or_cls_annotation.range(),
        &self_symbol_binding,
        replace_references_range,
        checker.semantic(),
    ));

    // (5) Determine the safety of the fixes as a whole
    let comment_ranges = checker.comment_ranges();

    let applicability = if other_edits
        .iter()
        .any(|edit| comment_ranges.intersects(edit.range()))
    {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    Ok(Some(Fix::applicable_edits(
        import_edit,
        other_edits,
        applicability,
    )))
}

/// Attempt to create an [`Edit`] that imports `Self`.
///
/// On Python <3.11, `Self` is imported from `typing_extensions`;
/// on Python >=3.11, it is imported from `typing`.
/// This is because it was added to the `typing` module on Python 3.11,
/// but is available from the backport package `typing_extensions` on all versions.
fn import_self(checker: &Checker, position: TextSize) -> Result<(Edit, String), ResolutionError> {
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

/// Returns a series of [`Edit`]s that modify all references to the given `typevar`.
///
/// Only references within `editable_range` will be modified.
/// This ensures that no edit in this series will overlap with other edits.
fn replace_typevar_usages_with_self<'a>(
    typevar: &'a TypeVar<'a>,
    self_or_cls_annotation_range: TextRange,
    self_symbol_binding: &'a str,
    editable_range: TextRange,
    semantic: &'a SemanticModel<'a>,
) -> impl Iterator<Item = Edit> + 'a {
    typevar
        .references(semantic)
        .map(Ranged::range)
        .filter(move |reference_range| editable_range.contains_range(*reference_range))
        .filter(move |reference_range| {
            !self_or_cls_annotation_range.contains_range(*reference_range)
        })
        .map(|reference_range| {
            Edit::range_replacement(self_symbol_binding.to_string(), reference_range)
        })
}

/// Create an [`Edit`] removing the `TypeVar` binding from the PEP 695 type parameter list.
///
/// Return `None` if we fail to find a `TypeVar` that matches the range of `typevar_binding`.
fn remove_pep695_typevar_declaration(
    type_params: &ast::TypeParams,
    custom_typevar: &TypeVar,
) -> Option<Edit> {
    if let [sole_typevar] = &**type_params {
        return (sole_typevar.name().range() == custom_typevar.range())
            .then(|| Edit::range_deletion(type_params.range));
    }

    let tvar_index = type_params
        .iter()
        .position(|param| param.name().range() == custom_typevar.range())?;

    let last_index = type_params.len() - 1;

    let deletion_range = if tvar_index < last_index {
        // def f[A, B, C](): ...
        //       ^^^ Remove this
        TextRange::new(custom_typevar.start(), type_params[tvar_index + 1].start())
    } else {
        // def f[A, B, C](): ...
        //           ^^^ Remove this
        TextRange::new(type_params[tvar_index - 1].end(), custom_typevar.end())
    };

    Some(Edit::range_deletion(deletion_range))
}

#[derive(Debug)]
struct TypeVar<'a> {
    name: &'a str,
    binding: &'a Binding<'a>,
}

impl TypeVar<'_> {
    const fn is_pep695_typevar(&self) -> bool {
        self.binding.kind.is_type_param()
    }

    fn references<'a>(
        &'a self,
        semantic: &'a SemanticModel<'a>,
    ) -> impl Iterator<Item = &'a ResolvedReference> + 'a {
        self.binding
            .references()
            .map(|reference_id| semantic.reference(reference_id))
    }
}

impl Ranged for TypeVar<'_> {
    fn range(&self) -> TextRange {
        self.binding.range()
    }
}
