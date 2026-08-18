use super::Bindings;
use crate::db::Db;
use crate::types::call::CallArguments;
use crate::types::{ClassBase, KnownClass, ProgramEnvironment, PropertyInstanceType, Type};
use itertools::Itertools;

impl<'db> Bindings<'db> {
    /// Replaces inherited property constructor results with their accessor-aware instance types.
    pub(super) fn evaluate_property_calls(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        call_arguments: &CallArguments<'_, 'db>,
    ) {
        for constructor in self.iter_constructor_items_mut() {
            let Some(instance) = constructor
                .constructed_instance_type()
                .as_nominal_instance()
            else {
                continue;
            };
            let class = instance.class(db, env);
            let property_base = match class.known(db) {
                Some(known @ (KnownClass::Property | KnownClass::EnumProperty)) => known,
                Some(_) => continue,
                None => {
                    let Some(base) = class
                        .iter_mro(db)
                        .filter_map(ClassBase::into_class)
                        .find_map(|base| {
                            base.known(db).filter(|known| {
                                matches!(known, KnownClass::Property | KnownClass::EnumProperty)
                            })
                        })
                    else {
                        continue;
                    };
                    base
                }
            };

            if class
                .iter_mro(db)
                .filter_map(ClassBase::into_class)
                .take_while(|base| !base.is_known(db, property_base))
                .any(|base| {
                    [
                        "__new__",
                        "__init__",
                        "__get__",
                        "__set__",
                        "__delete__",
                        "getter",
                        "setter",
                        "deleter",
                    ]
                    .into_iter()
                    .any(|name| !base.own_class_member(db, env, None, name).is_undefined())
                })
            {
                continue;
            }

            let property = {
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
                };
                let getter = accessor(0).filter(|ty| !ty.is_none(db));
                let setter = accessor(1).filter(|ty| !ty.is_none(db));
                let deleter = accessor(2).filter(|ty| !ty.is_none(db));

                Type::PropertyInstance(PropertyInstanceType::new_for_class(
                    db, class, getter, setter, deleter,
                ))
            };
            constructor.set_constructed_instance_type(property);
        }
    }
}
