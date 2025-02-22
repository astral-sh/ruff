from argparse import Namespace

a = Namespace()

(
    a
    # comment
    .b  # trailing comment
)

(
    a
    # comment
    .b  # trailing dot comment  # trailing identifier comment
)

(
    a
    # comment
    .b  # trailing identifier comment
)


(
    a
    # comment
    .  # trailing dot comment
    # in between
    b  # trailing identifier comment
)


a.aaaaaaaaaaaaaaaaaaaaa.lllllllllllllllllllllllllllloooooooooong.chaaaaaaaaaaaaaaaaaaaaaaiiiiiiiiiiiiiiiiiiiiiiinnnnnnnn.ooooooooooooooooooooooooofffffffff.aaaaaaaaaattr


# Test that we add parentheses around the outermost attribute access in an attribute
# chain if and only if we need them, that is if there are own line comments inside
# the chain.
x10 = (
    a
    .b
    # comment 1
    .  # comment 2
    # comment 3
    c
    .d
)

x11 = (
    a
    .b
    # comment 1
    .  # comment 2
    # comment 3
    c
    ).d

x12 = (
    (
    a
    .b
    # comment 1
    .  # comment 2
    # comment 3
    c
    )
    .d
)

x20 = (
    a
    .b
)
x21 = (
    # leading name own line
    a  # trailing name end-of-line
    .b
)
x22 = (
    a
    # outermost leading own line
    .b # outermost trailing end-of-line
)

x31 = (
    a
    # own line between nodes 1
    .b
)
x321 = (
    a
    . # end-of-line dot comment
    b
)
x322 = (
    a
    . # end-of-line dot comment 2
    b
    .c
)
x331 = (
    a.
    # own line between nodes 3
    b
)
x332 = (
    ""
    # own line between nodes
    .find
)

x8 = (
    (a + a)
    .b
)

x51 = (
    a.b.c
)
x52 = a.askjdfahdlskjflsajfadhsaf.akjdsf.aksjdlfadhaljsashdfljaf.askjdflhasfdlashdlfaskjfd.asdkjfksahdfkjafs
x53 = (
    a.askjdfahdlskjflsajfadhsaf.akjdsf.aksjdlfadhaljsashdfljaf.askjdflhasfdlashdlfaskjfd.asdkjfksahdfkjafs
)

x6 = (
    # Check assumption with enclosing nodes
    a.b
)

# Regression test for: https://github.com/astral-sh/ruff/issues/6181
(#
()).a

(
    (
        a # trailing end-of-line
        # trailing own-line
    ) # dangling before dot end-of-line
    .b # trailing end-of-line
)

(
    (
        a
    )
            # dangling before dot own-line
    .b
)

# Regression test for: https://github.com/astral-sh/ruff/issues/7370
result = (
    f(111111111111111111111111111111111111111111111111111111111111111111111111111111111)
    + 1
).bit_length()


# Regression test for https://github.com/astral-sh/ruff/issues/16151
result = (
   (await query_the_thing(mypy_doesnt_understand))  # type: ignore[x]
   .foo()
   .bar()
)

(
    (
        a # trailing end-of-line
        # trailing own-line
    ) # trailing closing parentheses
    # dangling before dot
    .b # trailing end-of-line
)
