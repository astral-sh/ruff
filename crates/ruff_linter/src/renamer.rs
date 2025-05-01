//! Code modification struct to support symbol renaming within a scope.

use anyhow::{anyhow, Result};
use itertools::Itertools;

use ruff_diagnostics::Edit;
use ruff_python_ast as ast;
use ruff_python_codegen::Stylist;
use ruff_python_semantic::{Binding, BindingKind, Scope, ScopeId, SemanticModel};
use ruff_python_stdlib::{builtins::is_python_builtin, keyword::is_keyword};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

pub(crate) struct Renamer;

impl Renamer {
    /// Rename a symbol (from `name` to `target`).
    ///
    /// ## How it works
    ///
    /// The renaming algorithm is as follows:
    ///
    /// 1. Determine the scope in which the rename should occur. This is typically the scope passed
    ///    in by the caller. However, if a symbol is `nonlocal` or `global`, then the rename needs
    ///    to occur in the scope in which the symbol is declared. For example, attempting to rename
    ///    `x` in `foo` below should trigger a rename in the module scope:
    ///
    /// ```python
    /// x = 1
    ///
    /// def foo():
    ///     global x
    ///     x = 2
    /// ```
    ///
    /// 1. Determine whether the symbol is rebound in another scope. This is effectively the inverse
    ///    of the previous step: when attempting to rename `x` in the module scope, we need to
    ///    detect that `x` is rebound in the `foo` scope. Determine every scope in which the symbol
    ///    is rebound, and add it to the set of scopes in which the rename should occur.
    ///
    /// 1. Start with the first scope in the stack. Take the first [`Binding`] in the scope, for the
    ///    given name. For example, in the following snippet, we'd start by examining the `x = 1`
    ///    binding:
    ///
    /// ```python
    /// if True:
    ///     x = 1
    ///     print(x)
    /// else:
    ///     x = 2
    ///     print(x)
    ///
    /// print(x)
    /// ```
    ///
    /// 1. Rename the [`Binding`]. In most cases, this is a simple replacement. For example,
    ///    renaming `x` to `y` above would require replacing `x = 1` with `y = 1`. After the
    ///    first replacement in the snippet above, we'd have:
    ///
    /// ```python
    /// if True:
    ///     y = 1
    ///     print(x)
    /// else:
    ///     x = 2
    ///     print(x)
    ///
    /// print(x)
    /// ```
    ///
    ///    Note that, when renaming imports, we need to instead rename (or add) an alias. For
    ///    example, to rename `pandas` to `pd`, we may need to rewrite `import pandas` to
    ///    `import pandas as pd`, rather than `import pd`.
    ///
    /// 1. Check to see if the binding is assigned to a known special call where the first argument
    ///    must be a string that is the same as the binding's name. For example,
    ///    `T = TypeVar("_T")` will be rejected by a type checker; only `T = TypeVar("T")` will do.
    ///    If it *is* one of these calls, we rename the relevant argument as well.
    ///
    /// 1. Rename every reference to the [`Binding`]. For example, renaming the references to the
    ///    `x = 1` binding above would give us:
    ///
    /// ```python
    /// if True:
    ///     y = 1
    ///     print(y)
    /// else:
    ///     x = 2
    ///     print(x)
    ///
    /// print(x)
    /// ```
    ///
    /// 1. Rename every delayed annotation. (See [`SemanticModel::delayed_annotations`].)
    ///
    /// 1. Repeat the above process for every [`Binding`] in the scope with the given name.
    ///    After renaming the `x = 2` binding, we'd have:
    ///
    /// ```python
    /// if True:
    ///     y = 1
    ///     print(y)
    /// else:
    ///     y = 2
    ///     print(y)
    ///
    /// print(y)
    /// ```
    ///
    /// 1. Repeat the above process for every scope in the stack.
    pub(crate) fn rename(
        name: &str,
        target: &str,
        scope: &Scope,
        semantic: &SemanticModel,
        stylist: &Stylist,
    ) -> Result<(Edit, Vec<Edit>)> {
        let mut edits = vec![];

        // Determine whether the symbol is `nonlocal` or `global`. (A symbol can't be both; Python
        // raises a `SyntaxError`.) If the symbol is `nonlocal` or `global`, we need to rename it in
        // the scope in which it's declared, rather than the current scope. For example, given:
        //
        // ```python
        // x = 1
        //
        // def foo():
        //     global x
        // ```
        //
        // When renaming `x` in `foo`, we detect that `x` is a global, and back out to the module
        // scope.
        let scope_id = scope.get_all(name).find_map(|binding_id| {
            let binding = semantic.binding(binding_id);
            match binding.kind {
                BindingKind::Global(_) => Some(ScopeId::global()),
                BindingKind::Nonlocal(_, scope_id) => Some(scope_id),
                _ => None,
            }
        });

        let scope = scope_id.map_or(scope, |scope_id| &semantic.scopes[scope_id]);
        edits.extend(Renamer::rename_in_scope(
            name, target, scope, semantic, stylist,
        ));

        // Find any scopes in which the symbol is referenced as `nonlocal` or `global`. For example,
        // given:
        //
        // ```python
        // x = 1
        //
        // def foo():
        //     global x
        //
        // def bar():
        //     global x
        // ```
        //
        // When renaming `x` in `foo`, we detect that `x` is a global, and back out to the module
        // scope. But we need to rename `x` in `bar` too.
        //
        // Note that it's impossible for a symbol to be referenced as both `nonlocal` and `global`
        // in the same program. If a symbol is referenced as `global`, then it must be defined in
        // the module scope. If a symbol is referenced as `nonlocal`, then it _can't_ be defined in
        // the module scope (because `nonlocal` can only be used in a nested scope).
        for scope_id in scope
            .get_all(name)
            .filter_map(|binding_id| semantic.rebinding_scopes(binding_id))
            .flatten()
            .dedup()
            .copied()
        {
            let scope = &semantic.scopes[scope_id];
            edits.extend(Renamer::rename_in_scope(
                name, target, scope, semantic, stylist,
            ));
        }

        // Deduplicate any edits.
        edits.sort();
        edits.dedup();

        let edit = edits
            .pop()
            .ok_or(anyhow!("Unable to rename any references to `{name}`"))?;

        Ok((edit, edits))
    }

    /// Rename a symbol in a single [`Scope`].
    fn rename_in_scope(
        name: &str,
        target: &str,
        scope: &Scope,
        semantic: &SemanticModel,
        stylist: &Stylist,
    ) -> Vec<Edit> {
        let mut edits = vec![];

        // Iterate over every binding to the name in the scope.
        for binding_id in scope.get_all(name) {
            let binding = semantic.binding(binding_id);

            // Rename the binding.
            if let Some(edit) = Renamer::rename_binding(binding, name, target) {
                edits.push(edit);

                if let Some(edit) =
                    Renamer::fixup_assigned_value(binding, semantic, stylist, name, target)
                {
                    edits.push(edit);
                }

                // Rename any delayed annotations.
                if let Some(annotations) = semantic.delayed_annotations(binding_id) {
                    edits.extend(annotations.iter().filter_map(|annotation_id| {
                        let annotation = semantic.binding(*annotation_id);
                        Renamer::rename_binding(annotation, name, target)
                    }));
                }

                // Rename the references to the binding.
                edits.extend(binding.references().map(|reference_id| {
                    let reference = semantic.reference(reference_id);
                    let replacement = {
                        if reference.in_dunder_all_definition() {
                            debug_assert!(!reference.range().is_empty());
                            let quote = stylist.quote();
                            format!("{quote}{target}{quote}")
                        } else {
                            target.to_string()
                        }
                    };
                    Edit::range_replacement(replacement, reference.range())
                }));
            }
        }

        // Deduplicate any edits. In some cases, a reference can be both a read _and_ a write. For
        // example, `x += 1` is both a read of and a write to `x`.
        edits.sort();
        edits.dedup();

        edits
    }

    /// If the r.h.s. of a call expression is a call expression,
    /// we may need to fixup some arguments passed to that call expression.
    ///
    /// It's impossible to do this entirely rigorously;
    /// we only special-case some common standard-library constructors here.
    ///
    /// For example, in this `TypeVar` definition:
    /// ```py
    /// from typing import TypeVar
    ///
    /// _T = TypeVar("_T")
    /// ```
    ///
    /// If we're renaming it from `_T` to `T`, we want this to be the end result:
    /// ```py
    /// from typing import TypeVar
    ///
    /// T = TypeVar("T")
    /// ```
    ///
    /// Not this, which a type checker will reject:
    /// ```py
    /// from typing import TypeVar
    ///
    /// T = TypeVar("_T")
    /// ```
    fn fixup_assigned_value(
        binding: &Binding,
        semantic: &SemanticModel,
        stylist: &Stylist,
        name: &str,
        target: &str,
    ) -> Option<Edit> {
        let statement = binding.statement(semantic)?;

        let (ast::Stmt::Assign(ast::StmtAssign { value, .. })
        | ast::Stmt::AnnAssign(ast::StmtAnnAssign {
            value: Some(value), ..
        })) = statement
        else {
            return None;
        };

        let ast::ExprCall {
            func, arguments, ..
        } = value.as_call_expr()?;

        let qualified_name = semantic.resolve_qualified_name(func)?;

        let name_argument = match qualified_name.segments() {
            ["collections", "namedtuple"] => arguments.find_argument_value("typename", 0),

            ["typing" | "typing_extensions", "TypeVar" | "ParamSpec" | "TypeVarTuple" | "NewType" | "TypeAliasType"] => {
                arguments.find_argument_value("name", 0)
            }

            ["enum", "Enum" | "IntEnum" | "StrEnum" | "ReprEnum" | "Flag" | "IntFlag"]
            | ["typing" | "typing_extensions", "NamedTuple" | "TypedDict"] => {
                arguments.find_positional(0)
            }

            ["builtins" | "", "type"] if arguments.len() == 3 => arguments.find_positional(0),

            _ => None,
        }?;

        let name_argument = name_argument.as_string_literal_expr()?;

        if name_argument.value.to_str() != name {
            return None;
        }

        let quote = stylist.quote();

        Some(Edit::range_replacement(
            format!("{quote}{target}{quote}"),
            name_argument.range(),
        ))
    }

    /// Rename a [`Binding`] reference.
    fn rename_binding(binding: &Binding, name: &str, target: &str) -> Option<Edit> {
        match &binding.kind {
            BindingKind::Import(_) | BindingKind::FromImport(_) => {
                if binding.is_alias() {
                    // Ex) Rename `import pandas as alias` to `import pandas as pd`.
                    Some(Edit::range_replacement(target.to_string(), binding.range()))
                } else {
                    // Ex) Rename `import pandas` to `import pandas as pd`.
                    Some(Edit::range_replacement(
                        format!("{name} as {target}"),
                        binding.range(),
                    ))
                }
            }
            BindingKind::SubmoduleImport(import) => {
                // Ex) Rename `import pandas.core` to `import pandas as pd`.
                let module_name = import.qualified_name.segments().first().unwrap();
                Some(Edit::range_replacement(
                    format!("{module_name} as {target}"),
                    binding.range(),
                ))
            }
            // Avoid renaming builtins and other "special" bindings.
            BindingKind::FutureImport | BindingKind::Builtin | BindingKind::Export(_) => None,
            // By default, replace the binding's name with the target name.
            BindingKind::Annotation
            | BindingKind::Argument
            | BindingKind::TypeParam
            | BindingKind::NamedExprAssignment
            | BindingKind::Assignment
            | BindingKind::BoundException
            | BindingKind::LoopVar
            | BindingKind::WithItemVar
            | BindingKind::Global(_)
            | BindingKind::Nonlocal(_, _)
            | BindingKind::ClassDefinition(_)
            | BindingKind::FunctionDefinition(_)
            | BindingKind::Deletion
            | BindingKind::ConditionalDeletion(_)
            | BindingKind::UnboundException(_) => {
                Some(Edit::range_replacement(target.to_string(), binding.range()))
            }
        }
    }
}

/// Enumeration of various ways in which a binding can shadow other variables
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum ShadowedKind {
    /// The variable shadows a global, nonlocal or local symbol
    Some,
    /// The variable shadows a builtin symbol
    BuiltIn,
    /// The variable shadows a keyword
    Keyword,
    /// The variable does not shadow any other symbols
    None,
}

impl ShadowedKind {
    /// Determines the kind of shadowing or conflict for the proposed new name of a given [`Binding`].
    ///
    /// This function is useful for checking whether or not the `target` of a [`Renamer::rename`]
    /// will shadow another binding.
    pub(crate) fn new(binding: &Binding, new_name: &str, checker: &Checker) -> ShadowedKind {
        // Check the kind in order of precedence
        if is_keyword(new_name) {
            return ShadowedKind::Keyword;
        }

        if is_python_builtin(
            new_name,
            checker.target_version().minor,
            checker.source_type.is_ipynb(),
        ) {
            return ShadowedKind::BuiltIn;
        }

        let semantic = checker.semantic();

        if !semantic.is_available_in_scope(new_name, binding.scope) {
            return ShadowedKind::Some;
        }

        if binding
            .references()
            .map(|reference_id| semantic.reference(reference_id).scope_id())
            .dedup()
            .any(|scope| !semantic.is_available_in_scope(new_name, scope))
        {
            return ShadowedKind::Some;
        }

        // Default to no shadowing
        ShadowedKind::None
    }

    /// Returns `true` if `self` shadows any global, nonlocal, or local symbol, keyword, or builtin.
    pub(crate) const fn shadows_any(self) -> bool {
        matches!(
            self,
            ShadowedKind::Some | ShadowedKind::BuiltIn | ShadowedKind::Keyword
        )
    }
}
