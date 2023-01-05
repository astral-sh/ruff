pub use bool_ops::{and_false, or_true};
pub use key_in_dict::{key_in_dict_compare, key_in_dict_for};
pub use unary_ops::{double_negation, negation_with_equal_op, negation_with_not_equal_op};
pub use yoda_conditions::yoda_conditions;

mod bool_ops;
mod key_in_dict;
mod unary_ops;
mod yoda_conditions;
