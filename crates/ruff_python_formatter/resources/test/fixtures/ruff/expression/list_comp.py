[i for i in []]

[i for i in [1,]]

[
    a  # a
    for # for
    c  # c
    in  # in
    e  # e
]

[
    # above a
    a  # a
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
]

[
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb + [dddddddddddddddddd, eeeeeeeeeeeeeeeeeee]
    for
    ccccccccccccccccccccccccccccccccccccccc,
    ddddddddddddddddddd, [eeeeeeeeeeeeeeeeeeeeee, fffffffffffffffffffffffff]
    in
    eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeffffffffffffffffffffffffggggggggggggggggggggghhhhhhhhhhhhhhothermoreeand_even_moreddddddddddddddddddddd
    if
    fffffffffffffffffffffffffffffffffffffffffff < gggggggggggggggggggggggggggggggggggggggggggggg < hhhhhhhhhhhhhhhhhhhhhhhhhh
    if
    gggggggggggggggggggggggggggggggggggggggggggg
]

# Regression tests for https://github.com/astral-sh/ruff/issues/5911
selected_choices = [
    str(v) for v in value if str(v) not in self.choices.field.empty_values
]

selected_choices = [
    str(v)
    for vvvvvvvvvvvvvvvvvvvvvvv in value if str(v) not in self.choices.field.empty_values
]

# Tuples with BinOp
[i for i in (aaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb, ccccccccccccccccccccc)]
[(aaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb, ccccccccccccccccccccc) for i in b]

a = [
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    for f in bbbbbbbbbbbbbbb
    if f not in ccccccccccc
]

a = [
    [1, 2, 3,]
    for f in bbbbbbbbbbbbbbb
    if f not in ccccccccccc
]

aaaaaaaaaaaaaaaaaaaaa = [
    o for o in self.registry.values if o.__class__ is not ModelAdmin
]

[
    task
    for head_name in head_nodes
    if (task := self.get_task_by_name(head_name))
    if not None
]

[
    f(x, y,)
    for head_name in head_nodes if head_name
]

[
    f(x, y)
    for head_name in f(x, y,) if head_name
]

[
    x
    for (x, y,) in z if head_name
]

[
     1 for components in  # pylint: disable=undefined-loop-variable
     b +  # integer 1 may only have decimal 01-09
     c  # negative decimal
]

# Parenthesized targets and iterators.
[x for (x) in y]
[x for x in (y)]


# Leading expression comments:
y = [
    a
    for (
        # comment
        a
    ) in (
        # comment
        x
    )
    if (
        # asdasd
        "askldaklsdnmklasmdlkasmdlkasmdlkasmdasd"
        != "as,mdnaskldmlkasdmlaksdmlkasdlkasdm"
        and "zxcm,.nzxclm,zxnckmnzxckmnzxczxc" != "zxcasdasdlmnasdlknaslkdnmlaskdm"
    )
    if (
        # comment
        x
    )
]

# Tuple target:
y = [
    a
    for
        # comment
        a, b
     in x
    if  True
]


y = [
    a
    for (
        # comment
        a, b
    )
    in x
    if  True
]


y = [
    a
    for
        # comment
        a
    in
        # comment
        x
    if
        # asdasd
        "askldaklsdnmklasmdlkasmdlkasmdlkasmdasd"
        != "as,mdnaskldmlkasdmlaksdmlkasdlkasdm"
        and "zxcm,.nzxclm,zxnckmnzxckmnzxczxc" != "zxcasdasdlmnasdlknaslkdnmlaskdm"
    if
        # comment
        x
]
