if not aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb:
    pass

a = True
not a

b = 10
-b
+b

## Leading operand comments

if not (
    # comment
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
):
    pass


if ~(
    # comment
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb):
    pass

if -(
    # comment
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb):
    pass


if +(
    # comment
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb):
    pass

if (
    not
    # comment
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
):
    pass


if (
    ~
    # comment
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb):
    pass

if (
    -
    # comment
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb):
    pass


if (
    +
    # comment
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb):
    pass

## Parentheses

if (
    # unary comment
    not
    # operand comment
    (
        # comment
        aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
        + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
    )
):
    pass

if (not (
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
)
):
    pass

if aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa & (not (
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
)
):
    pass

if (
    not (
            aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
            + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
        )
        & aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
):
    pass


## Trailing operator comments

if (
    not # comment
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
):
    pass


if (
    ~ # comment
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
):
    pass

if (
    - # comment
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
):
    pass


if (
    + # comment
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    + bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
):
    pass


## Varia

if not \
    a:
    pass

# Regression test for: https://github.com/astral-sh/ruff/issues/5338
if a and not aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa & aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:
    pass

if (
  not
  # comment
  a):
    pass

if (
  not  # comment
  a):
    pass

# Regression test for: https://github.com/astral-sh/ruff/issues/7423
if True:
    if True:
        if True:
            if not yn_question(
                Fore.RED
                + "WARNING: Removing listed files. Do you really want to continue. yes/n)? "
            ):
                pass

# Regression test for: https://github.com/astral-sh/ruff/issues/7448
x = (
    # a
    not # b
    # c
    ( # d
        # e
        True
    )
)

# Regression test for: https://github.com/astral-sh/ruff/issues/8090
if "root" not in (
    long_tree_name_tree.split("/")[0]
    for long_tree_name_tree in really_really_long_variable_name
):
    msg = "Could not find root. Please try a different forest."
    raise ValueError(msg)

# Regression for https://github.com/astral-sh/ruff/issues/8183
def foo():
    while (
        not (aaaaaaaaaaaaaaaaaaaaa(bbbbbbbb, ccccccc)) and dddddddddd < eeeeeeeeeeeeeee
    ):
        pass

def foo():
    while (
        not (aaaaaaaaaaaaaaaaaaaaa[bbbbbbbb, ccccccc]) and dddddddddd < eeeeeeeeeeeeeee
    ):
        pass
