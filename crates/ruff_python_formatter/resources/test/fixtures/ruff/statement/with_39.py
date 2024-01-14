if True:
    with (
        anyio.CancelScope(shield=True)
        if get_running_loop()
        else contextlib.nullcontext()
    ):
        pass


# Black avoids parenthesizing the with because it can make all with items fit by just breaking
# around parentheses. We don't implement this optimisation because it makes it difficult to see where
# the different context managers start and end.
with cmd, xxxxxxxx.some_kind_of_method(
    some_argument=[
        "first",
        "second",
        "third",
    ]
) as cmd, another, and_more as x:
    pass

# Avoid parenthesizing single item context managers when splitting after the parentheses (can_omit_optional_parentheses)
# is sufficient
with xxxxxxxx.some_kind_of_method(
    some_argument=[
        "first",
        "second",
        "third",
    ]
).another_method(): pass

if True:
    with (
        anyio.CancelScope(shield=True)
        if get_running_loop()
        else contextlib.nullcontext()
    ):
        pass


# Black avoids parentheses here because it can make the entire with
# header fit without requiring parentheses to do so.
# We don't implement this optimisation because it very difficult to see where
# the different context managers start or end.
with cmd, xxxxxxxx.some_kind_of_method(
    some_argument=[
        "first",
        "second",
        "third",
    ]
) as cmd, another, and_more as x:
    pass

# Avoid parenthesizing single item context managers when splitting after the parentheses
# is sufficient
with xxxxxxxx.some_kind_of_method(
    some_argument=[
        "first",
        "second",
        "third",
    ]
).another_method(): pass

# Parenthesize the with items if it makes them fit. Breaking the binary expression isn't
# necessary because the entire items fit just into the 88 character limit.
with aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb as c:
    pass


# Black parenthesizes this binary expression but also preserves the parentheses of the first with-item.
# It does so because it prefers splitting already parenthesized context managers, even if it leads to more parentheses
# like in this case.
with (
    (
        aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
    ) as b,
    c as d,
):
    pass

if True:
    with anyio.CancelScope(shield=True) if get_running_loop() else contextlib.nullcontext():
        pass

with (aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb as c):
    pass


