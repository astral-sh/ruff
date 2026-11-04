[a for graph_path_expression in refined_constraint.condition_as_predicate.variables]
[
    a
    for graph_path_expression in refined_constraint.condition_as_predicate.variables
]
[
    a
    for graph_path_expression
    in refined_constraint.condition_as_predicate.variables
]
[
    a
    for graph_path_expression in (
        refined_constraint.condition_as_predicate.variables
    )
]

[
    (foobar_very_long_key, foobar_very_long_value)
    for foobar_very_long_key, foobar_very_long_value in foobar_very_long_dictionary.items()
]

# Don't split the `in` if it's not too long
lcomp3 = [
    element.split("\n", 1)[0]
    for element in collection.select_elements()
    # right
    if element is not None
]

# Don't remove parens around ternaries
expected = [i for i in (a if b else c)]

# Don't remove parens around lambdas; without them the trailing comprehension
# clauses would be parsed as part of the lambda body, producing invalid code
lambda_iter = [i for i in (lambda: 0) if i]
lambda_iter_no_clause = [i for i in (lambda x: x)]

# A lambda or ternary iterable must keep at least one pair of parens, but any
# redundant nested pairs are collapsed down to a single pair
double_lambda_iter = [i for i in ((lambda: 0)) if i]
double_lambda_no_clause = [i for i in ((lambda x: x))]
double_ternary = [i for i in ((a if b else c))]
triple_lambda = [i for i in (((lambda: 0))) if i]
[a for a in (yield b)]
[a for a in (((yield b)))]

# Nested arrays
# First in will not be split because it would still be too long
[[
    x
    for x in bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
    for y in xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
]]

# Multiple comprehensions, only split the second `in`
graph_path_expressions_in_local_constraint_refinements = [
    graph_path_expression
    for refined_constraint in self._local_constraint_refinements.values()
    if refined_constraint is not None
    for graph_path_expression in refined_constraint.condition_as_predicate.variables
]

# Dictionary comprehensions
dict_with_really_long_names = {
    really_really_long_key_name: an_even_longer_really_really_long_key_value
    for really_really_long_key_name, an_even_longer_really_really_long_key_value in really_really_really_long_dict_name.items()
}
{
    key_with_super_really_long_name: key_with_super_really_long_name
    for key_with_super_really_long_name in dictionary_with_super_really_long_name
}
{
    key_with_super_really_long_name: key_with_super_really_long_name
    for key_with_super_really_long_name
    in dictionary_with_super_really_long_name
}
{
    key_with_super_really_long_name: key_with_super_really_long_name
    for key in (
        dictionary
    )
}

# Short `in`s with delimiters should retain up to one level of parens
filtered_trait_instructions: list[str] = [
    trait for trait in ((traits_instructions or [])) if trait
]
filtered_trait_instructions: list[str] = [
    trait for trait in (traits_instructions or []) if trait
]
filtered_trait_instructions: list[str] = [
    trait for trait in traits_instructions or [] if trait
]

# Short `in`s without delimiters (or only dots) should remove parens
filtered_trait_instructions: list[str] = [
    trait for trait in (traits_instructions) if trait
]
filtered_trait_instructions: list[str] = [
    trait for trait in traits_instructions if trait
]
filtered_trait_instructions: list[str] = [
    trait for trait in (traits.instructions) if trait
]
filtered_trait_instructions: list[str] = [
    trait for trait in traits.instructions if trait
]

# comments
[
    a
    for a in
    # foo0
    (
        # foo1
        (b)
        # foo2
    )
    # foo3
]

[
    a
    for a in
    # foo4
    ((b))
]

[
    a
    for a in (
        # foo5
        (b)
    )
]

[
    a
    for a in (
        (b)
        # foo6
    )
]

[
    a
    for a in ((b))
    # foo7
]
