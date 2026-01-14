//! A home for deferred checks that must be done after the `TypeInferenceBuilder` has done an initial
//! inference pass over the whole scope.

pub(super) mod dynamic_class;
pub(super) mod dynamic_dataclass;
pub(super) mod final_variable;
pub(super) mod function;
pub(super) mod overloaded_function;
pub(super) mod static_class;
pub(super) mod typeguard;
