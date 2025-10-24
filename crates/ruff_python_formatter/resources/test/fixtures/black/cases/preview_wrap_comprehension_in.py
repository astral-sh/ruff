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
