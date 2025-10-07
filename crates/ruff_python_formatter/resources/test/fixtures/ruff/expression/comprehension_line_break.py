# Test file for comprehension-line-break option
# Tests both "auto" (default) and "preserve" modes

# Dictionary comprehensions
dict_comp = {
    obj["key"]: obj["value"]
    for obj
    in get_fields()
    if obj["key"] != "name"
}

# This one should fit on one line in auto mode
short_dict = {
    k: v
    for k, v in items()
}

# List comprehensions
list_comp = [
    item * 2
    for item
    in range(10)
    if item % 2 == 0
]

# Set comprehensions
set_comp = {
    value.upper()
    for value
    in collection
    if value
}

# Generator expressions
gen_expr = (
    x ** 2
    for x
    in numbers
    if x > 0
)

# Nested comprehensions
nested = [
    [
        (i, j)
        for j
        in range(3)
    ]
    for i
    in range(5)
]

# Complex multiline with multiple clauses
complex_comp = [
    process(item)
    for sublist
    in data
    for item
    in sublist
    if validate(item)
    if not skip(item)
]

# Single-line comprehensions should remain single-line
single_line_dict = {k: v for k, v in items()}
single_line_list = [x for x in range(10)]
single_line_set = {x for x in collection}
single_line_gen = (x for x in numbers)

# Comprehensions with comments
commented = {
    # Process key
    key.lower():
    # Process value
    value.strip()
    for key, value
    in pairs
    # Only valid entries
    if is_valid(key, value)
}

# Comprehensions with long expressions that must break
long_expr = [
    very_long_function_name_that_causes_line_break(item, parameter1, parameter2, parameter3)
    for item in very_long_iterable_name_that_also_causes_issues
    if complex_condition_check(item, threshold=100)
]