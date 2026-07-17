use ruff_db::parsed::parsed_module;
use ruff_python_ast::{Expr, name::Name};
use ruff_python_stdlib::identifiers::is_mangled_private;
use ty_module_resolver::{KnownModule, file_to_module};
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::semantic_index;

use crate::Db;
use crate::types::infer::{infer_deferred_types, nearest_enclosing_class};
use crate::types::{StaticClassLiteral, Type, definition_expression_type};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum AutoAttribs {
    Enabled,
    Disabled,
    Infer,
}

pub(in crate::types) fn field_specifiers_reference_attrs<'db>(
    db: &'db dyn Db,
    field_specifiers: &[Type<'db>],
) -> bool {
    field_specifiers
        .iter()
        .any(|specifier| is_attrs_field_specifier(db, *specifier))
}

pub(in crate::types) fn is_attrs_field_specifier<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    let Some(function) = ty.as_function_literal() else {
        return false;
    };

    matches!(
        (
            file_to_module(db, function.file(db)).and_then(|module| module.known(db)),
            function.name(db).as_str(),
        ),
        (Some(KnownModule::Attr), "attrib") | (Some(KnownModule::Attrs), "field")
    )
}

pub(in crate::types) fn is_legacy_field_specifier<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    let Some(function) = ty.as_function_literal() else {
        return false;
    };

    function.name(db) == "attrib"
        && file_to_module(db, function.file(db))
            .is_some_and(|module| module.is_known(db, KnownModule::Attr))
}

pub(in crate::types) fn field_specifier<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Option<(Type<'db>, Option<Type<'db>>)> {
    let module = parsed_module(db, definition.file(db)).load(db);
    let value = match definition.kind(db) {
        DefinitionKind::Assignment(assignment) => assignment.value(&module),
        DefinitionKind::AnnotatedAssignment(assignment) => assignment.value(&module)?,
        _ => return None,
    };
    let call = value.as_call_expr()?;
    let callee = definition_expression_type(db, definition, &call.func);

    if !is_attrs_field_specifier(db, callee) {
        return None;
    }

    let field_type = if is_legacy_field_specifier(db, callee) {
        call.arguments.find_keyword("type").map(|keyword| {
            if keyword.value.is_string_literal_expr() {
                infer_deferred_types(db, definition).expression_type(&keyword.value)
            } else {
                definition_expression_type(db, definition, &keyword.value).project_type_form(db)
            }
        })
    } else {
        None
    };

    Some((
        definition_expression_type(db, definition, value),
        field_type,
    ))
}

pub(in crate::types) fn auto_attribs<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> AutoAttribs {
    let semantic = semantic_index(db, class.file(db));
    let Some(class_stmt) = semantic
        .scope(class.body_scope(db).file_scope_id(db))
        .node()
        .as_class()
    else {
        return AutoAttribs::Infer;
    };
    let class_definition = semantic.expect_single_definition(class_stmt);
    let module = parsed_module(db, class.file(db)).load(db);
    let class_stmt = class_stmt.node(&module);

    for decorator in &class_stmt.decorator_list {
        let call = decorator.expression.as_call_expr();
        let decorator_callable = call.map_or(&decorator.expression, |call| &call.func);
        let decorator_ty = definition_expression_type(db, class_definition, decorator_callable);
        let Some(function) = decorator_ty.as_function_literal() else {
            continue;
        };
        let module = file_to_module(db, function.file(db)).and_then(|module| module.known(db));
        if !matches!(module, Some(KnownModule::Attr | KnownModule::Attrs)) {
            continue;
        }

        if let Some(auto_attribs) =
            call.and_then(|call| call.arguments.find_keyword("auto_attribs"))
        {
            let auto_attribs =
                definition_expression_type(db, class_definition, &auto_attribs.value).bool(db);
            if auto_attribs.is_always_true() {
                return AutoAttribs::Enabled;
            }
            if auto_attribs.is_always_false() {
                return AutoAttribs::Disabled;
            }
        }

        let decorator_name = match decorator_callable {
            Expr::Attribute(attribute) => attribute.attr.as_str(),
            Expr::Name(name) => name.id.as_str(),
            _ => function.name(db).as_str(),
        };
        return match decorator_name {
            "dataclass" => AutoAttribs::Enabled,
            "s" | "attrs" | "attributes" => AutoAttribs::Disabled,
            "define" | "mutable" | "frozen" => AutoAttribs::Infer,
            _ if function.name(db) == "attrs" => AutoAttribs::Disabled,
            _ => AutoAttribs::Infer,
        };
    }

    AutoAttribs::Infer
}

pub(in crate::types) fn init_parameter_name<'db>(
    db: &'db dyn Db,
    field_name: &Name,
    first_declaration: Option<Definition<'db>>,
) -> Name {
    let mangled = if is_mangled_private(field_name) {
        first_declaration.and_then(|definition| {
            let semantic = semantic_index(db, definition.file(db));
            let defining_class = nearest_enclosing_class(db, semantic, definition.scope(db))?;
            let class_name = defining_class.name(db).trim_start_matches('_');
            (!class_name.is_empty()).then(|| format!("_{class_name}{field_name}"))
        })
    } else {
        None
    };
    let effective = mangled.as_deref().unwrap_or(field_name);
    let stripped = effective.trim_start_matches('_');

    if stripped.is_empty() {
        field_name.clone()
    } else {
        Name::new(stripped)
    }
}
