a1 = 1 if True else 2

a2 = "this is a very long text that will make the group break to check that parentheses are added" if True else 2

# These comment should be kept in place
b1 = (
    # We return "a" ...
    "a" # that's our True value
    # ... if this condition matches ...
    if True # that's our test
    # ... otherwise we return "b"
    else "b" # that's our False value
)

# These only need to be stable, bonus is we also keep the order
c1 = (
    "a" # 1
    if # 2
    True # 3
    else # 4
    "b" # 5
)
c2 = (
    "a" # 1
    # 2
    if # 3
    # 4
    True # 5
    # 6
    else # 7
    # 8
    "b" # 9
)

# regression test: parentheses outside the expression ranges interfering with finding
# the `if` and `else` token finding
d1 = [
    ("a") if # 1
    ("b") else # 2
    ("c")
]

e1 = (
    a
    if True # 1
    else b
    if False # 2
    else c
)


# Flattening nested if-expressions.
def something():
    clone._iterable_class = (
        NamedValuesListIterable
        if named
        else FlatValuesListIterable
        if flat
        else ValuesListIterable
    )


def something():
    clone._iterable_class = (
        (NamedValuesListIterable
        if named
        else FlatValuesListIterable)
        if flat
        else ValuesListIterable
    )


def something():
    clone._iterable_class = (
        NamedValuesListIterable
        if named
        else (FlatValuesListIterable
        if flat
        else ValuesListIterable)
    )


def something():
    clone._iterable_class = (
        NamedValuesListIterable
        if named
        else FlatValuesListIterable(1,)
        if flat
        else ValuesListIterable
    )


def something():
    clone._iterable_class = (
        NamedValuesListIterable
        if named
        else FlatValuesListIterable + FlatValuesListIterable + FlatValuesListIterable + FlatValuesListIterable
        if flat
        else ValuesListIterable
    )


def something():
    clone._iterable_class = (
        NamedValuesListIterable
        if named
        else (FlatValuesListIterable + FlatValuesListIterable + FlatValuesListIterable + FlatValuesListIterable
        if flat
        else ValuesListIterable)
    )
