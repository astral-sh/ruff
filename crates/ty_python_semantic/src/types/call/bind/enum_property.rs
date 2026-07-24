use super::Bindings;
use crate::db::Db;
use crate::types::call::CallArguments;
use crate::types::{KnownClass, PropertyInstanceType, Type};
use itertools::Itertools;

impl<'db> Bindings<'db> {
    /// Replaces constructed `enum.property` instances with the property type derived from their
    /// accessor arguments.
    pub(super) fn evaluate_enum_property_calls(
        &mut self,
        db: &'db dyn Db,
        call_arguments: &CallArguments<'_, 'db>,
    ) {
        let property_instance =
            |getter: Option<Type<'db>>, setter: Option<Type<'db>>, deleter: Option<Type<'db>>| {
                Type::PropertyInstance(PropertyInstanceType::new_enum_property(
                    db,
                    getter.filter(|ty| !ty.is_none(db)),
                    setter.filter(|ty| !ty.is_none(db)),
                    deleter.filter(|ty| !ty.is_none(db)),
                ))
            };

        // TODO: Preserve subclasses of `enum.property`. `PropertyInstanceType` currently records
        // only a known property class, so this rewrite collapses subclass instances to
        // `enum.property`.
        for constructor in self.iter_constructor_items_mut() {
            if !constructor
                .constructed_instance_type()
                .is_instance_of(db, KnownClass::EnumProperty)
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
                property_instance(accessor(0), accessor(1), accessor(2))
            };
            constructor.set_constructed_instance_type(property);
        }
    }
}
