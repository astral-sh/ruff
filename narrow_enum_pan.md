# Target
finish the todos in crates/ty_python_semantic/resources/mdtest/narrow/conditionals/in.md
* ## enums
* ## Union with enum and `int`

# Idea from reviewers
I think the highest priority follow-up would be to add enum support and address those TODOs. If you are interested, I think this would not be too hard. In principle it is just adding enums (along with bool and LiteralString) in is_union_of_single_valued and is_union_with_single_valued, and in the union logic in evaluate_expr_in and evaluate_expr_not_in. The one wrinkle is that we only consider enums single-valued if they don't override __eq__ or __ne__. This logic is currently buried inside the EnumLiteral branch of Type::is_single_valued; we would need to pull it out into a top-level Type::overrides_eq_or_ne method, since we would also need to call it from is_union_of_single_valued etc. (Should probably also add some tests showing we don't do this narrowing for an enum class that does override __eq__ or __ne__.)

# My plan
* add a function Type::is_simple_enum() -> bool (similar to is_bool and is_literal_string) that returns true for Enum types that don't override __eq__ or __ne__
  * If don't know how to check if a type overrides __eq__ or __ne__, I can look at how Type::is_single_valued does it
* modify is_union_of_single_valued and is_union_with_single_valued to call is_simple_enum
* check the todos in crates/ty_python_semantic/resources/mdtest/narrow/conditionals/in.md, do them one by one
  * use `cargo insta test -p ty_python_semantic -- mdtest__narrow_conditionals_in` to test
  * modify evaluate_expr_in and evaluate_expr_not_in in narrow.rs to handle enums, if necessary
