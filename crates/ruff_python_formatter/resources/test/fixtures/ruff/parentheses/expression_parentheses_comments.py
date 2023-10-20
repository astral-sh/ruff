list_with_parenthesized_elements1 = [
    # comment leading outer
    (
        # comment leading inner
            1 + 2 # comment trailing inner
    ) # comment trailing outer
]

list_with_parenthesized_elements2 = [
    # leading outer
    (1 + 2)
]
list_with_parenthesized_elements3 = [
    # leading outer
    (1 + 2) # trailing outer
]
list_with_parenthesized_elements4 = [
    # leading outer
    (1 + 2), # trailing outer
]
list_with_parenthesized_elements5 = [
    (1), # trailing outer
    (2), # trailing outer
]

nested_parentheses1 = (
    (
        (
            1
        ) # i
    ) # j
) # k
nested_parentheses2 = [
    (
        (
            (
                1
            ) # i
            # i2
        ) # j
        # j2
    ) # k
    # k2
]
nested_parentheses3 = (
    ( # a
        ( # b
            1
        ) # i
    ) # j
) # k
nested_parentheses4 = [
    # a
    ( # b
        # c
        ( # d
            # e
            ( #f
                1
            ) # i
            # i2
        ) # j
        # j2
    ) # k
    # k2
]


x = (
    # unary comment
    not
    # in-between comment
    (
        # leading inner
        "a"
    ),
    not # in-between comment
    (
        # leading inner
        "b"
    ),
    not
    (  # in-between comment
        # leading inner
        "c"
    ),
    # 1
    not # 2
    ( # 3
        # 4
        "d"
    )
)

if (
    # unary comment
    not
    # in-between comment
    (
            # leading inner
            1
    )
):
    pass

# Make sure we keep a inside the parentheses
# https://github.com/astral-sh/ruff/issues/7892
x = (
    # a
    ( # b
        1
    )
)
