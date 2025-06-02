use crate::checkers::ast::Checker;
use crate::fix::edits::remove_unused_imports;
use crate::importer::ImportRequest;
use crate::rules::numpy::helpers::{AttributeSearcher, ImportSearcher};
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::name::QualifiedNameBuilder;
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{Expr, ExprAttribute, ExprName, StmtTry};
use ruff_python_semantic::Exceptions;
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::{MemberNameImport, NameImport};
use ruff_text_size::Ranged;
use ruff_text_size::TextRange;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Replacement {
    // There's no replacement or suggestion other than removal
    None,
    // The attribute name of a class has been changed.
    AttrName(&'static str),
    // Additional information. Used when there's replacement but they're not direct mapping.
    Message(&'static str),
    // Symbols updated in Airflow 3 with replacement
    // e.g., `airflow.datasets.Dataset` to `airflow.sdk.Asset`
    AutoImport {
        module: &'static str,
        name: &'static str,
    },
    // Symbols updated in Airflow 3 with only module changed. Used when we want to match multiple names.
    // e.g., `airflow.configuration.as_dict | get` to `airflow.configuration.conf.as_dict | get`
    SourceModuleMoved {
        module: &'static str,
        name: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProviderReplacement {
    None,
    AutoImport {
        module: &'static str,
        name: &'static str,
        provider: &'static str,
        version: &'static str,
    },
    SourceModuleMovedToProvider {
        module: &'static str,
        name: String,
        provider: &'static str,
        version: &'static str,
    },
}

pub(crate) fn is_guarded_by_try_except(
    expr: &Expr,
    module: &str,
    name: &str,
    semantic: &SemanticModel,
) -> bool {
    match expr {
        Expr::Attribute(_) => {
            if !semantic.in_exception_handler() {
                return false;
            }
            let Some(try_node) = semantic
                .current_statements()
                .find_map(|stmt| stmt.as_try_stmt())
            else {
                return false;
            };
            let suspended_exceptions = Exceptions::from_try_stmt(try_node, semantic);
            if !suspended_exceptions.contains(Exceptions::ATTRIBUTE_ERROR) {
                return false;
            }
            try_block_contains_undeprecated_attribute(try_node, module, name, semantic)
        }
        Expr::Name(ExprName { id, .. }) => {
            let Some(binding_id) = semantic.lookup_symbol(id.as_str()) else {
                return false;
            };
            let binding = semantic.binding(binding_id);
            if !binding.is_external() {
                return false;
            }
            if !binding.in_exception_handler() {
                return false;
            }
            let Some(try_node) = binding.source.and_then(|import_id| {
                semantic
                    .statements(import_id)
                    .find_map(|stmt| stmt.as_try_stmt())
            }) else {
                return false;
            };
            let suspended_exceptions = Exceptions::from_try_stmt(try_node, semantic);
            if !suspended_exceptions
                .intersects(Exceptions::IMPORT_ERROR | Exceptions::MODULE_NOT_FOUND_ERROR)
            {
                return false;
            }
            try_block_contains_undeprecated_import(try_node, module, name)
        }
        _ => false,
    }
}

/// Given an [`ast::StmtTry`] node, does the `try` branch of that node
/// contain any [`ast::ExprAttribute`] nodes that indicate the airflow
/// member is being accessed from the non-deprecated location?
fn try_block_contains_undeprecated_attribute(
    try_node: &StmtTry,
    module: &str,
    name: &str,
    semantic: &SemanticModel,
) -> bool {
    let undeprecated_qualified_name = {
        let mut builder = QualifiedNameBuilder::default();
        for part in module.split('.') {
            builder.push(part);
        }
        builder.push(name);
        builder.build()
    };
    let mut attribute_searcher = AttributeSearcher::new(undeprecated_qualified_name, semantic);
    attribute_searcher.visit_body(&try_node.body);
    attribute_searcher.found_attribute
}

/// Given an [`ast::StmtTry`] node, does the `try` branch of that node
/// contain any [`ast::StmtImportFrom`] nodes that indicate the airflow
/// member is being imported from the non-deprecated location?
fn try_block_contains_undeprecated_import(try_node: &StmtTry, module: &str, name: &str) -> bool {
    let mut import_searcher = ImportSearcher::new(module, name);
    import_searcher.visit_body(&try_node.body);
    import_searcher.found_import
}

/// Check whether the segments corresponding to the fully qualified name points to a symbol that's
/// either a builtin or coming from one of the providers in Airflow.
///
/// The pattern it looks for are:
/// - `airflow.providers.**.<module>.**.*<symbol_suffix>` for providers
/// - `airflow.<module>.**.*<symbol_suffix>` for builtins
///
/// where `**` is one or more segments separated by a dot, and `*` is one or more characters.
///
/// Examples for the above patterns:
/// - `airflow.providers.google.cloud.secrets.secret_manager.CloudSecretManagerBackend` (provider)
/// - `airflow.secrets.base_secrets.BaseSecretsBackend` (builtin)
pub(crate) fn is_airflow_builtin_or_provider(
    segments: &[&str],
    module: &str,
    symbol_suffix: &str,
) -> bool {
    match segments {
        ["airflow", "providers", rest @ ..] => {
            if let (Some(pos), Some(last_element)) =
                (rest.iter().position(|&s| s == module), rest.last())
            {
                // Check that the module is not the last element i.e., there's a symbol that's
                // being used from the `module` that ends with `symbol_suffix`.
                pos + 1 < rest.len() && last_element.ends_with(symbol_suffix)
            } else {
                false
            }
        }

        ["airflow", first, rest @ ..] => {
            if let Some(last) = rest.last() {
                *first == module && last.ends_with(symbol_suffix)
            } else {
                false
            }
        }

        _ => false,
    }
}

/// Return the [`ast::ExprName`] at the head of the expression, if any.
pub(crate) fn match_head(value: &Expr) -> Option<&ExprName> {
    match value {
        Expr::Attribute(ExprAttribute { value, .. }) => value.as_name_expr(),
        Expr::Name(name) => Some(name),
        _ => None,
    }
}

/// Return the [`Fix`] that imports the new name and updates where the import is referenced.
/// This is used for cases that member name has changed.
/// (e.g., `airflow.datasts.Dataset` to `airflow.sdk.Asset`)
pub(crate) fn generate_import_edit(
    expr: &Expr,
    checker: &Checker,
    module: &str,
    name: &str,
    ranged: TextRange,
) -> Option<Fix> {
    let (import_edit, _) = checker
        .importer()
        .get_or_import_symbol(
            &ImportRequest::import_from(module, name),
            expr.start(),
            checker.semantic(),
        )
        .ok()?;
    let replacement_edit = Edit::range_replacement(name.to_string(), ranged.range());
    Some(Fix::safe_edits(import_edit, [replacement_edit]))
}

/// Return the [`Fix`] that remove the original import and import the same name with new path.
/// This is used for cases that member name has not changed.
/// (e.g., `airflow.operators.pig_operator.PigOperator` to `airflow.providers.apache.pig.hooks.pig.PigCliHook`)
pub(crate) fn generate_remove_and_runtime_import_edit(
    expr: &Expr,
    checker: &Checker,
    module: &str,
    name: &str,
) -> Option<Fix> {
    let head = match_head(expr)?;
    let semantic = checker.semantic();
    let binding = semantic
        .resolve_name(head)
        .or_else(|| checker.semantic().lookup_symbol(&head.id))
        .map(|id| checker.semantic().binding(id))?;
    let stmt = binding.statement(semantic)?;
    let remove_edit = remove_unused_imports(
        std::iter::once(name),
        stmt,
        None,
        checker.locator(),
        checker.stylist(),
        checker.indexer(),
    )
    .ok()?;
    let import_edit = checker.importer().add_import(
        &NameImport::ImportFrom(MemberNameImport::member(
            (*module).to_string(),
            name.to_string(),
        )),
        expr.start(),
    );

    Some(Fix::unsafe_edits(remove_edit, [import_edit]))
}
