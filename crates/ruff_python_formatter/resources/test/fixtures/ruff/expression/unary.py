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

# Regression: https://github.com/astral-sh/ruff/issues/5338
if a and not aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa & aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:
    ...

if (
  not
  # comment
  a):
    ...

if (
  not  # comment
  a):
    ...

# Regression test for: https://github.com/astral-sh/ruff/issues/7423
if True:
    if True:
        if True:
            if not yn_question(
                Fore.RED
                + "WARNING: Removing listed files. Do you really want to continue. yes/n)? "
            ):
                pass
