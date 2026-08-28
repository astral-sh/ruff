use super::{Bindings, ConstructorCallableKind};
use crate::db::Db;
use crate::types::call::CallArguments;
use crate::types::{
    ClassBase, KnownClass, MemberLookupPolicy, ProgramEnvironment, PropertyInstanceType, Type,
    is_property_method,
};
use itertools::Itertools;

impl<'db> Bindings<'db> {
    /// Retains the accessors and nominal class when the property initializer is inherited.
    pub(super) fn evaluate_property_calls(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        call_arguments: &CallArguments<'_, 'db>,
    ) {
        for constructor in self.iter_constructor_items_mut() {
            if constructor.context().kind() != ConstructorCallableKind::Init {
                continue;
            }
            let function = match constructor.callable().callable_type {
                Type::BoundMethod(method) => method.function(db),
                Type::FunctionLiteral(function) => function,
                _ => continue,
            };
            if function.name(db) != "__init__" || !is_property_method(db, env, function) {
                continue;
            }
            let Some(instance) = constructor
                .constructed_instance_type()
                .as_nominal_instance()
            else {
                continue;
            };
            let class = instance.class(db, env);
            // The first class that defines the accessor storage must be a known property class.
            let inherits_accessors = class
                .iter_mro(db)
                .filter_map(ClassBase::into_class)
                .find_map(|base| {
                    if matches!(
                        base.known(db),
                        Some(KnownClass::Property | KnownClass::EnumProperty)
                    ) {
                        Some(true)
                    } else if ["fget", "fset", "fdel"]
                        .into_iter()
                        .any(|name| !base.own_class_member(db, env, None, name).is_undefined())
                    {
                        Some(false)
                    } else {
                        None
                    }
                });
            if inherits_accessors != Some(true) {
                continue;
            }
            // Property-specific protocol and override checks use the stored accessors directly.
            // A subclass that changes descriptor behavior must instead use ordinary descriptors.
            if ["__get__", "__set__", "__delete__"]
                .into_iter()
                .any(|name| {
                    class
                        .class_member(db, env, name, MemberLookupPolicy::default())
                        .place
                        .ignore_possibly_undefined()
                        .and_then(Type::as_function_literal)
                        .is_none_or(|function| !is_property_method(db, env, function))
                })
            {
                continue;
            }
            let Ok((_, overload)) = constructor.callable().matching_overloads().exactly_one()
            else {
                continue;
            };
            let accessor = |parameter_index| {
                call_arguments
                    .iter()
                    .zip(overload.argument_matches())
                    .find_map(|((_, argument_types), argument_matches)| {
                        let parameter = argument_matches
                            .parameters
                            .iter()
                            .find(|parameter| parameter.index == parameter_index)?;
                        parameter
                            .argument_type
                            .or_else(|| argument_types.get_default())
                    })
                    .filter(|ty| !ty.is_none(db))
            };
            let property = Type::PropertyInstance(PropertyInstanceType::new_with_class(
                db,
                class,
                accessor(0),
                accessor(1),
                accessor(2),
            ));
            constructor.set_constructed_instance_type(property);
        }
    }
}
