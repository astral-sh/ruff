first_item, second_item = (
    some_looooooooong_module.some_looooooooooooooong_function_name(
        first_argument, second_argument, third_argument
    )
)

some_dict["with_a_long_key"] = (
    some_looooooooong_module.some_looooooooooooooong_function_name(
        first_argument, second_argument, third_argument
    )
)

# Make sure it works when the RHS only has one pair of (optional) parens.
first_item, second_item = (
    some_looooooooong_module.SomeClass.some_looooooooooooooong_variable_name
)

some_dict["with_a_long_key"] = (
    some_looooooooong_module.SomeClass.some_looooooooooooooong_variable_name
)

# Make sure chaining assignments work.
first_item, second_item, third_item, forth_item = m["everything"] = (
    some_looooooooong_module.some_looooooooooooooong_function_name(
        first_argument, second_argument, third_argument
    )
)

# Make sure when the RHS's first split at the non-optional paren fits,
# we split there instead of the outer RHS optional paren.
first_item, second_item = some_looooooooong_module.some_loooooog_function_name(
    first_argument, second_argument, third_argument
)

(
    first_item,
    second_item,
    third_item,
    forth_item,
    fifth_item,
    last_item_very_loooooong,
) = some_looooooooong_module.some_looooooooooooooong_function_name(
    first_argument, second_argument, third_argument
)

(
    first_item,
    second_item,
    third_item,
    forth_item,
    fifth_item,
    last_item_very_loooooong,
) = everything = some_looooong_function_name(
    first_argument, second_argument, third_argument
)


# Make sure unsplittable type ignore won't be moved.
some_kind_of_table[some_key] = util.some_function(  # type: ignore  # noqa: E501
    some_arg
).intersection(pk_cols)

some_kind_of_table[
    some_key
] = lambda obj: obj.some_long_named_method()  # type: ignore  # noqa: E501

some_kind_of_table[
    some_key  # type: ignore  # noqa: E501
] = lambda obj: obj.some_long_named_method()


# Make when when the left side of assignment plus the opening paren "... = (" is
# exactly line length limit + 1, it won't be split like that.
xxxxxxxxx_yyy_zzzzzzzz[
    xx.xxxxxx(x_yyy_zzzzzz.xxxxx[0]), x_yyy_zzzzzz.xxxxxx(xxxx=1)
] = 1


# Right side of assignment contains un-nested pairs of inner parens.
some_kind_of_instance.some_kind_of_map[a_key] = (
    isinstance(some_var, SomeClass)
    and table.something_and_something != table.something_else
) or (
    isinstance(some_other_var, BaseClass) and table.something != table.some_other_thing
)

# Multiple targets
a = b = (
    ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc
)

a = b = c = d = e = f = g = (
    hhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhh
) = i = j = (
    kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk
)

a = (
    bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
) = c

a = (
    bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
) = (
    cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc
) = ddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd
