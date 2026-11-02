use ruff_python_ast::{self as ast, helpers::is_dunder, name::UnqualifiedName};
use ty_module_resolver::file_to_module;
use ty_python_core::{scope::NodeWithScopeKind, semantic_index};

use crate::dunder_all::dunder_all_names;
use crate::types::context::InferContext;
use crate::types::diagnostic::{
    MISSING_RETURN_TYPE_CLASS_METHOD, MISSING_RETURN_TYPE_PRIVATE_FUNCTION,
    MISSING_RETURN_TYPE_PUBLIC_FUNCTION, MISSING_RETURN_TYPE_SPECIAL_METHOD,
    MISSING_RETURN_TYPE_STATIC_METHOD,
};
use crate::types::function::FunctionType;

pub(super) fn check_missing_return_annotation<'db>(
    context: &InferContext<'db, '_>,
    function_type: FunctionType<'db>,
) {
    let db = context.db();
    let literal = function_type.literal(db);
    let last_definition = literal.last_definition;
    if last_definition.has_explicit_return_annotation(db) || literal.has_separate_implementation(db)
    {
        return;
    }

    if ![
        &MISSING_RETURN_TYPE_PUBLIC_FUNCTION,
        &MISSING_RETURN_TYPE_PRIVATE_FUNCTION,
        &MISSING_RETURN_TYPE_SPECIAL_METHOD,
        &MISSING_RETURN_TYPE_STATIC_METHOD,
        &MISSING_RETURN_TYPE_CLASS_METHOD,
    ]
    .into_iter()
    .any(|rule| context.is_lint_enabled(rule))
    {
        return;
    }

    let function = last_definition.node(db, context.file(), context.module());
    let is_method = last_definition.body_scope(db).is_method_scope(db);
    let (lint, kind) = if is_method && last_definition.is_classmethod(db) {
        (&MISSING_RETURN_TYPE_CLASS_METHOD, "class method")
    } else if is_method && last_definition.is_staticmethod(db) {
        (&MISSING_RETURN_TYPE_STATIC_METHOD, "static method")
    } else if is_method && is_dunder(&function.name) {
        (&MISSING_RETURN_TYPE_SPECIAL_METHOD, "special method")
    } else if is_private(context, function, is_method) {
        (&MISSING_RETURN_TYPE_PRIVATE_FUNCTION, "private function")
    } else {
        (&MISSING_RETURN_TYPE_PUBLIC_FUNCTION, "public function")
    };

    let name = &function.name;
    let Some(builder) = context.report_lint(lint, name) else {
        return;
    };
    builder.into_diagnostic(format_args!(
        "Missing return type annotation for {kind} `{name}`"
    ));
}

fn is_private(
    context: &InferContext<'_, '_>,
    function: &ast::StmtFunctionDef,
    is_method: bool,
) -> bool {
    let db = context.db();
    if function.name.starts_with('_') {
        return true;
    }
    if is_method && function.decorator_list.iter()
        .filter_map(|decorator| UnqualifiedName::from_expr(&decorator.expression))
        .any(|name| matches!(name.segments(), [property, "setter" | "deleter"] if *property == function.name.as_str()))
    {
        return true;
    }

    let file = context.program_file();
    let index = semantic_index(db, file);
    let mut exported_name = function.name.as_str();
    for (_, scope) in index.ancestor_scopes(context.scope().file_scope_id(db)) {
        match scope.node() {
            NodeWithScopeKind::Function(_) => return true,
            NodeWithScopeKind::Class(class) => {
                exported_name = class.node(context.module()).name.as_str();
                if exported_name.starts_with('_') {
                    return true;
                }
            }
            _ => {}
        }
    }

    file_to_module(db, file.resolver_file(db)).map_or_else(
        || {
            context
                .file()
                .path(db)
                .as_system_path()
                .and_then(|p| p.file_stem())
                .is_some_and(|stem| stem.starts_with('_'))
        },
        |module| module.name(db).is_private(),
    ) || dunder_all_names(db, file).is_some_and(|names| !names.contains(exported_name))
}
