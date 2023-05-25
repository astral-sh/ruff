pub(crate) use builtin_argument_shadowing::{builtin_argument_shadowing, BuiltinArgumentShadowing};
pub(crate) use builtin_attribute_shadowing::{
    builtin_attribute_shadowing, BuiltinAttributeShadowing,
};
pub(crate) use builtin_variable_shadowing::{builtin_variable_shadowing, BuiltinVariableShadowing};

mod builtin_argument_shadowing;
mod builtin_attribute_shadowing;
mod builtin_variable_shadowing;
