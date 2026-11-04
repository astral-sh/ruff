# Regression test for https://github.com/psf/black/issues/4514
# Black used to split between a variable and its comparator (`if t / not in`)
# whenever the RHS was a bracketed expression that had to break. With the fix,
# the comparator stays attached to its left operand and the bracket explodes
# instead. The same shape appears outside comprehensions (inside `if`/`elif`,
# `assert`, parenthesized expressions, etc.) and is fixed in all of them.

# Comprehension `if` with a magic-trailing-comma set on the RHS. This is the
# original case from the issue.
x = [
    t
    for t in y
    if t not in {LongNameOne, LongNameTwo, LongNameThree,}
]

# Different comparator (`<`) and a magic-trailing-comma call on the RHS.
ranked = [
    t
    for t in y
    if t < SomeFunction(LongNameOne, LongNameTwo, LongNameThree,)
]

# Boolean operator before the comparator: the `or` splits, and the remaining
# `t in {...,}` defers to the bracket explosion instead of splitting at `in`.
combined = [
    t
    for t in y
    if some_short_predicate or t in {LongNameOne, LongNameTwo, LongNameThree,}
]

# No magic trailing comma: the RHS is long enough that Black has to break it
# anyway. The comparator should stay with `if t`.
no_magic = [
    t
    for t in y
    if t not in some_very_long_function_name(argument_one, argument_two, argument_three, argument_four)
]

# `if` inside an `and` chain with a magic-trailing-comma tuple on the RHS.
if (
    is_scalar(value)
    and self.dtype in (np.dtype("float64"), np.dtype("float32"), np.dtype("object"),)
    and (limit is not None or inplace)
):
    do_thing()

# `assert` with `is` and a call-chain RHS that has to break.
assert (
    bool is _AnnotationExtractor(attr.fields(C).x.converter.__call__).get_return_type()
)

# `assert` with `in` and a chained call as RHS.
assert (
    f"{existing_section}\n{init_basic_toml_no_readme}" in pyproject_file.read_text(encoding="utf-8")
)
