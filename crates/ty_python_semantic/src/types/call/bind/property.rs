use super::Bindings;
use crate::db::Db;
use crate::types::call::CallArguments;
use crate::types::{ClassBase, KnownClass, ProgramEnvironment, PropertyInstanceType, Type};
use itertools::Itertools;

impl<'db> Bindings<'db> {
    /// Preserves property subclasses and their accessor signatures when construction uses the
    /// inherited descriptor behavior.
    ///
    /// ```python
    /// class CustomProperty(property): ...
    ///
    /// class Example:
    ///     @CustomProperty
    ///     def value(self) -> int: ...
    /// ```
    ///
    /// `Example.value` retains its `CustomProperty` type, while `Example().value` uses the
    /// getter's `int` return type. Subclasses with custom construction, descriptor methods, or
    /// accessor decorators retain ordinary descriptor inference instead.
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

            // The accessor passed to a property subclass is not necessarily the accessor
            // stored by the resulting descriptor. For example:
            //
            // class ReplacingProperty(property):
            //     def __init__(self, getter):
            //         super().__init__(lambda _: "replacement")
            //
            // class Example:
            //     @ReplacingProperty
            //     def value(self) -> int:
            //         return 1
            //
            // Here, `Example().value` is a string despite the original getter returning
            // an integer. A custom `__get__` can likewise ignore the stored getter:
            //
            // class ConstantProperty(property):
            //     def __get__(self, instance, owner=None):
            //         return "replacement"
            //
            // Accessor decorators can also replace the descriptor entirely:
            //
            // class ReplacingSetter(property):
            //     def setter(self, setter):
            //         return property(lambda _: "replacement")
            //
            // Check the subclass and its intermediate bases, but stop before the known
            // property base: its own methods are exactly the behavior modeled below.
            // Leave subclasses that override any of these methods to ordinary descriptor
            // inference rather than assuming they retain the supplied accessors.
            if class
                .iter_mro(db)
                .filter_map(ClassBase::into_class)
                .take_while(|base| !base.is_known(db, property_base))
                .any(|base| {
                    [
                        // Constructors can replace or discard the supplied accessors.
                        "__new__",
                        "__init__",
                        // Descriptor methods control attribute reads, writes, and deletion.
                        "__get__",
                        "__set__",
                        "__delete__",
                        // Accessor decorators can construct a different descriptor.
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
            let property = PropertyInstanceType::new_for_class(
                db,
                class,
                accessor(0),
                accessor(1),
                accessor(2),
            );
            constructor.set_constructed_instance_type(Type::PropertyInstance(property));
        }
    }
}
