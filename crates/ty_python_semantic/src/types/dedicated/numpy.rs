use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ty_module_resolver::file_to_module;
use ty_python_core::ast_ids::HasScopedUseId;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::semantic_index;

use crate::types::definition_expression_type;
use crate::types::definition_resolution::{
    ImportAliasResolution, ResolvedDefinition, definitions_for_attribute, definitions_for_name,
    resolve_definition,
};
use crate::types::infer::{implicit_alias_parameters, infer_implicit_alias_type};
use crate::types::{ClassSpecialization, ClassType, SubclassOfInner, SubclassOfType, Type};
use crate::{Db, ProgramEnvironment};

/// NumPy's older stubs describe scalar classes such as `int64` as generic aliases such as
/// `signedinteger[_64Bit]`. Identify these aliases and return a class-object type instead.
///
/// The scalar class is a subclass of the specialized base, not the base itself. Keep that
/// distinction so that checking against `int64` does not exclude every `signedinteger`.
/// This special case can be removed once all supported NumPy versions include
/// <https://github.com/numpy/numpy/pull/33052>, which replaces the aliases with class definitions.
pub(in crate::types) fn scalar_class<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    name: &str,
    alias: ClassSpecialization<'db>,
) -> Option<Type<'db>> {
    let origin_name = match name {
        "int8" | "int16" | "int32" | "int64" | "byte" | "short" | "intc" | "intp" | "int_"
        | "long" | "longlong" => "signedinteger",
        "uint8" | "uint16" | "uint32" | "uint64" | "ubyte" | "ushort" | "uintc" | "uintp"
        | "uint" | "ulong" | "ulonglong" => "unsignedinteger",
        "float16" | "float32" | "float64" | "float96" | "float128" | "half" | "single"
        | "double" | "longdouble" | "float_" | "longfloat" => "floating",
        "complex64" | "complex128" | "complex192" | "complex256" | "csingle" | "cdouble"
        | "clongdouble" | "complex_" | "cfloat" | "clongfloat" | "longcomplex" => "complexfloating",
        _ => return None,
    };

    if !definition.file_scope(db).is_global() || alias.origin(db).name(db) != origin_name {
        return None;
    }

    let module = file_to_module(db, definition.program_file(db).resolver_file(db))?;
    match module.name(db).as_str() {
        "numpy" if definition.file(db).is_stub(db) => {}
        // NumPy keeps these platform-dependent definitions in a Python module so its mypy
        // plugin can remove the ones that are unavailable on the target platform.
        "numpy._typing._extended_precision"
            if matches!(name, "float96" | "float128" | "complex192" | "complex256") => {}
        _ => return None,
    }

    let origin_module = file_to_module(db, alias.origin(db).program_file(db).resolver_file(db))?;
    if origin_module.name(db).as_str() != "numpy" {
        return None;
    }

    Some(SubclassOfType::from(
        db,
        &ProgramEnvironment::from_file(definition.program_file(db)),
        ClassType::Generic(alias),
    ))
}

/// Return the specialization of a possible scalar-class alias in older NumPy stubs.
///
/// This only identifies candidates: a variable with this type is not necessarily a scalar alias.
/// Annotation inference must still check the definition's right-hand side as a type expression.
fn scalar_class_specialization<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<ClassSpecialization<'db>> {
    let Type::SubclassOf(subclass) = ty else {
        return None;
    };
    let SubclassOfInner::Class(ClassType::Generic(alias)) = subclass.subclass_of() else {
        return None;
    };
    let origin = alias.origin(db);
    if !matches!(
        origin.name(db).as_str(),
        "signedinteger" | "unsignedinteger" | "floating" | "complexfloating"
    ) {
        return None;
    }
    let module = file_to_module(db, origin.program_file(db).resolver_file(db))?;
    (module.name(db).as_str() == "numpy").then_some(alias)
}

/// Recover a scalar alias's annotation meaning without accepting arbitrary class-valued variables.
pub(in crate::types) fn scalar_class_alias<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    definition: Definition<'db>,
) -> Option<ClassSpecialization<'db>> {
    let alias = scalar_class_specialization(db, ty)?;
    scalar_class_alias_impl(db, alias, definition)
}

#[salsa::tracked(returns(copy))]
fn scalar_class_alias_impl<'db>(
    db: &'db dyn Db,
    alias: ClassSpecialization<'db>,
    definition: Definition<'db>,
) -> Option<ClassSpecialization<'db>> {
    let mut definition = definition;
    let env = ProgramEnvironment::from_file(definition.program_file(db));
    if definition.kind(db).is_import() {
        let name = definition.name(db);
        let definitions = resolve_definition(
            db,
            &env,
            definition,
            name.as_deref(),
            ImportAliasResolution::ResolveAliases,
        );
        let [resolved] = definitions.as_slice() else {
            return None;
        };
        definition = resolved.definition()?;
    }
    let module = parsed_module(db, definition.program_file(db).python_file(db)).load(db);
    let value = definition.kind(db).value(&module)?;
    if !matches!(
        value,
        ast::Expr::Name(_) | ast::Expr::Attribute(_) | ast::Expr::Subscript(_)
    ) {
        return None;
    }
    match definition.kind(db) {
        DefinitionKind::Assignment(_) => {}
        DefinitionKind::AnnotatedAssignment(assignment)
            if definition_expression_type(db, definition, assignment.annotation(&module))
                .is_typealias_special_form() => {}
        _ => return None,
    }
    let parameters = implicit_alias_parameters(db, definition);
    let annotation = infer_implicit_alias_type(db, definition, parameters)
        .ty
        .ok()?;
    (annotation == Type::instance(db, &env, alias.into())).then_some(alias)
}

/// Preserve the specialized base when a scalar alias is used in a class definition.
pub(in crate::types) fn scalar_class_base<'db>(
    db: &'db dyn Db,
    class_definition: Definition<'db>,
    expression: &ast::Expr,
    ty: Type<'db>,
) -> Option<Type<'db>> {
    scalar_class_specialization(db, ty)?;
    let file = class_definition.program_file(db);
    let definitions = match expression {
        ast::Expr::Name(name) => {
            let index = semantic_index(db, file);
            let scope = index.expression_scope_id(expression);
            let definitions = index
                .use_def_map(scope)
                .bindings_at_use(name.scoped_use_id(db, file))
                .filter_map(|binding| binding.binding.definition())
                .collect::<Vec<_>>();
            if definitions.is_empty() {
                definitions_for_name(
                    db,
                    scope.to_scope_id(db, file),
                    &name.id,
                    ImportAliasResolution::ResolveAliases,
                )
                .iter()
                .filter_map(ResolvedDefinition::definition)
                .collect()
            } else {
                definitions
            }
        }
        ast::Expr::Attribute(attribute) => definitions_for_attribute(
            db,
            &ProgramEnvironment::from_file(file),
            definition_expression_type(db, class_definition, &attribute.value),
            &attribute.attr,
        )
        .iter()
        .filter_map(ResolvedDefinition::definition)
        .collect(),
        _ => return None,
    };
    let [definition] = definitions.as_slice() else {
        return None;
    };
    scalar_class_alias(db, ty, *definition).map(Type::GenericAlias)
}
