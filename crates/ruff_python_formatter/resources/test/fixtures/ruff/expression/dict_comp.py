{i: i for i in []}

{i: i for i in [1,]}

{
    a: a  # a
    for # for
    c  # c
    in  # in
    e  # e
}

{
    # above a
    a: a  # a
    # above for
    for # for
    # above c
    c  # c
    # above in
    in  # in
    # above e
    e  # e
    # above if
    if  # if
    # above f
    f  # f
    # above if2
    if  # if2
    # above g
    g  # g
}

{
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb + [dddddddddddddddddd, eeeeeeeeeeeeeeeeeee]: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    for
    ccccccccccccccccccccccccccccccccccccccc,
    ddddddddddddddddddd, [eeeeeeeeeeeeeeeeeeeeee, fffffffffffffffffffffffff]
    in
    eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeffffffffffffffffffffffffggggggggggggggggggggghhhhhhhhhhhhhhothermoreeand_even_moreddddddddddddddddddddd
    if
    fffffffffffffffffffffffffffffffffffffffffff < gggggggggggggggggggggggggggggggggggggggggggggg < hhhhhhhhhhhhhhhhhhhhhhhhhh
    if
    gggggggggggggggggggggggggggggggggggggggggggg
}

# Useful for tuple target (see https://github.com/astral-sh/ruff/issues/5779#issuecomment-1637614763)
{k: v for a, a, a, a, a, a, a, a, a, a, [a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a]  in this_is_a_very_long_variable_which_will_cause_a_trailing_comma_which_breaks_the_comprehension}
{k: v for a, a, a, a, a, a, a, a, a, a, (a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a,)  in this_is_a_very_long_variable_which_will_cause_a_trailing_comma_which_breaks_the_comprehension}

# Leading
{  # Leading
    k: v  # Trailing
    for a, a, a, a, a, a, a, a, a, a, (  # Trailing
        a,
        a,
        a,
        a,
        a,
        a,
        a,
        a,
        a,  # Trailing
        a,
        a,
        a,
        a,
        a,
        a,
        a,
        a,
        a,
        a,
        a,  # Trailing
    ) in this_is_a_very_long_variable_which_will_cause_a_trailing_comma_which_breaks_the_comprehension  # Trailing
}  # Trailing
# Trailing


# Regression tests for https://github.com/astral-sh/ruff/issues/5911
selected_choices = {
    k: str(v) for v in value if str(v) not in self.choices.field.empty_values
}

selected_choices = {
    k: str(v)
    for vvvvvvvvvvvvvvvvvvvvvvv in value if str(v) not in self.choices.field.empty_values
}

{
    k: v
    for (  # foo

    x, aaaayaaaayaaaayaaaayaaaayaaaayaaaayaaaayaaaayaaaayaaaayaaaayaaaayaaaay) in z
}

a = {
    k: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    for f in bbbbbbbbbbbbbbb
    if f not in ccccccccccc
}

a = {
    k: [1, 2, 3,]
    for f in bbbbbbbbbbbbbbb
    if f not in ccccccccccc
}

aaaaaaaaaaaaaaaaaaaaa = {
    k: o for o in self.registry.values if o.__class__ is not ModelAdmin
}

# Comments between keys and values.
query = {
    key:
    # queries => map(pluck("fragment")) => flatten()
    [
        clause
        for kf_pair in queries
        for clause in kf_pair["fragment"]
    ]
    for key, queries in self._filters.items()
}

query = {
    key:  # queries => map(pluck("fragment")) => flatten()
    [
        clause
        for kf_pair in queries
        for clause in kf_pair["fragment"]
    ]
    for key, queries in self._filters.items()
}

query = {
    key: (
    # queries => map(pluck("fragment")) => flatten()
    [
        clause
        for kf_pair in queries
        for clause in kf_pair["fragment"]
    ]
    )
    for key, queries in self._filters.items()
}

query = {
    key: (  # queries => map(pluck("fragment")) => flatten()
    [
        clause
        for kf_pair in queries
        for clause in kf_pair["fragment"]
    ]
    )
    for key, queries in self._filters.items()
}

query = {
    (
        key
        # queries => map(pluck("fragment")) => flatten()
    ): [
        clause
        for kf_pair in queries
        for clause in kf_pair["fragment"]
    ]
    for key, queries in self._filters.items()
}

query = {
    (
        key # queries => map(pluck("fragment")) => flatten()
    ): [
        clause
        for kf_pair in queries
        for clause in kf_pair["fragment"]
    ]
    for key, queries in self._filters.items()
}
