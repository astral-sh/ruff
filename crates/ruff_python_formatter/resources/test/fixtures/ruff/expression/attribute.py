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


aaaaaaaaaaaaaaaaaaaaa.lllllllllllllllllllllllllllloooooooooong.chaaaaaaaaaaaaaaaaaaaaaaiiiiiiiiiiiiiiiiiiiiiiinnnnnnnn.ooooooooooooooooooooooooofffffffff.aaaaaaaaaattr


# Test that we add parentheses around the outermost attribute access in an attribute
# chain if and only if we need them, that is if there are own line comments inside
# the chain.
x1 = (
    a1
    .a2
    # a
    .  # b
    # c
    a3
    .a4
)
x20 = (
    a
    .b
)
x21 = (
    a
    # trailing name own line
    .b
)
x22 = (
    ""
    # leading
    .b
)
x23 = (
    # leading
    a
    .b
)
x24 = (
    a # trailing name end-of-line
    .b
)
x31 = (
    a
    .# a
    b
)
x32 = (
    a
    .# a
    b
    .c
)
x4 = (
    a.
    # a
    b
)
x5 = (
    a.b.c
)
x61 = askjdfahdlskjflsajfadhsaf.akjdsf.aksjdlfadhaljsashdfljaf.askjdflhasfdlashdlfaskjfd.asdkjfksahdfkjafs
x62 = (
    askjdfahdlskjflsajfadhsaf.akjdsf.aksjdlfadhaljsashdfljaf.askjdflhasfdlashdlfaskjfd.asdkjfksahdfkjafs
)
