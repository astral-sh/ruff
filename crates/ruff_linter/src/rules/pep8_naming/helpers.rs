use itertools::Itertools;
use ruff_python_ast::call_path::collect_call_path;
use ruff_python_ast::{self as ast, Arguments, Expr, Stmt};

use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::str::{is_cased_lowercase, is_cased_uppercase};

pub(super) fn is_camelcase(name: &str) -> bool {
    !is_cased_lowercase(name) && !is_cased_uppercase(name) && !name.contains('_')
}

pub(super) fn is_mixed_case(name: &str) -> bool {
    !is_cased_lowercase(name)
        && name
            .strip_prefix('_')
            .unwrap_or(name)
            .chars()
            .next()
            .map_or_else(|| false, char::is_lowercase)
}

pub(super) fn is_acronym(name: &str, asname: &str) -> bool {
    name.chars().filter(|c| c.is_uppercase()).join("") == asname
}

/// Returns `true` if the statement is an assignment to a named tuple.
pub(super) fn is_named_tuple_assignment(stmt: &Stmt, semantic: &SemanticModel) -> bool {
    let Stmt::Assign(ast::StmtAssign { value, .. }) = stmt else {
        return false;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
        return false;
    };
    semantic.resolve_call_path(func).is_some_and(|call_path| {
        matches!(call_path.as_slice(), ["collections", "namedtuple"])
            || semantic.match_typing_call_path(&call_path, "NamedTuple")
    })
}

/// Returns `true` if the statement is an assignment to a `TypedDict`.
pub(super) fn is_typed_dict_assignment(stmt: &Stmt, semantic: &SemanticModel) -> bool {
    if !semantic.seen_typing() {
        return false;
    }

    let Stmt::Assign(ast::StmtAssign { value, .. }) = stmt else {
        return false;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
        return false;
    };
    semantic.match_typing_expr(func, "TypedDict")
}

/// Returns `true` if the statement is an assignment to a `TypeVar` or `NewType`.
pub(super) fn is_type_var_assignment(stmt: &Stmt, semantic: &SemanticModel) -> bool {
    if !semantic.seen_typing() {
        return false;
    }

    let Stmt::Assign(ast::StmtAssign { value, .. }) = stmt else {
        return false;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
        return false;
    };
    semantic.resolve_call_path(func).is_some_and(|call_path| {
        semantic.match_typing_call_path(&call_path, "TypeVar")
            || semantic.match_typing_call_path(&call_path, "NewType")
    })
}

/// Returns `true` if the statement is an assignment to a `TypeAlias`.
pub(super) fn is_type_alias_assignment(stmt: &Stmt, semantic: &SemanticModel) -> bool {
    match stmt {
        Stmt::AnnAssign(ast::StmtAnnAssign { annotation, .. }) => {
            semantic.match_typing_expr(annotation, "TypeAlias")
        }
        Stmt::TypeAlias(_) => true,
        _ => false,
    }
}

/// Returns `true` if the statement is an assignment to a `TypedDict`.
pub(super) fn is_typed_dict_class(arguments: Option<&Arguments>, semantic: &SemanticModel) -> bool {
    if !semantic.seen_typing() {
        return false;
    }

    arguments.is_some_and(|arguments| {
        arguments
            .args
            .iter()
            .any(|base| semantic.match_typing_expr(base, "TypedDict"))
    })
}

/// Returns `true` if a statement appears to be a dynamic import of a Django model.
///
/// For example, in Django, it's common to use `get_model` to access a model dynamically, as in:
/// ```python
/// def migrate_existing_attachment_data(
///     apps: StateApps, schema_editor: BaseDatabaseSchemaEditor
/// ) -> None:
///     Attachment = apps.get_model("zerver", "Attachment")
/// ```
pub(super) fn is_django_model_import(name: &str, stmt: &Stmt, semantic: &SemanticModel) -> bool {
    fn match_model_import(name: &str, expr: &Expr, semantic: &SemanticModel) -> bool {
        let Expr::Call(ast::ExprCall {
            func, arguments, ..
        }) = expr
        else {
            return false;
        };

        if arguments.is_empty() {
            return false;
        }

        // Match against, e.g., `apps.get_model("zerver", "Attachment")`.
        if let Some(call_path) = collect_call_path(func.as_ref()) {
            if matches!(call_path.as_slice(), [.., "get_model"]) {
                if let Some(argument) =
                    arguments.find_argument("model_name", arguments.args.len().saturating_sub(1))
                {
                    if let Some(string_literal) = argument.as_string_literal_expr() {
                        if string_literal.value.to_str() == name {
                            return true;
                        }
                    } else {
                        return true;
                    }
                }
            }
        }

        // Match against, e.g., `import_string("zerver.models.Attachment")`.
        if let Some(call_path) = semantic.resolve_call_path(func.as_ref()) {
            if matches!(
                call_path.as_slice(),
                ["django", "utils", "module_loading", "import_string"]
            ) {
                if let Some(argument) = arguments.find_argument("dotted_path", 0) {
                    if let Some(string_literal) = argument.as_string_literal_expr() {
                        if let Some((.., model)) = string_literal.value.to_str().rsplit_once('.') {
                            if model == name {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        false
    }

    match stmt {
        Stmt::AnnAssign(ast::StmtAnnAssign {
            value: Some(value), ..
        }) => match_model_import(name, value.as_ref(), semantic),
        Stmt::Assign(ast::StmtAssign { value, .. }) => {
            match_model_import(name, value.as_ref(), semantic)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_acronym, is_camelcase, is_mixed_case};

    #[test]
    fn test_is_camelcase() {
        assert!(is_camelcase("Camel"));
        assert!(is_camelcase("CamelCase"));
        assert!(!is_camelcase("camel"));
        assert!(!is_camelcase("camel_case"));
        assert!(!is_camelcase("CAMEL"));
        assert!(!is_camelcase("CAMEL_CASE"));
    }

    #[test]
    fn test_is_mixed_case() {
        assert!(is_mixed_case("mixedCase"));
        assert!(is_mixed_case("mixed_Case"));
        assert!(is_mixed_case("_mixed_Case"));
        assert!(!is_mixed_case("mixed_case"));
        assert!(!is_mixed_case("MIXED_CASE"));
        assert!(!is_mixed_case(""));
        assert!(!is_mixed_case("_"));
    }

    #[test]
    fn test_is_acronym() {
        assert!(is_acronym("AB", "AB"));
        assert!(is_acronym("AbcDef", "AD"));
        assert!(!is_acronym("AbcDef", "Ad"));
        assert!(!is_acronym("AbcDef", "AB"));
    }
}
