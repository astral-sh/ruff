a = 10
# fmt: off

# more format

def test(): ...


# fmt: on

b =   20
# Sequence of trailing comments that toggle between format on and off. The sequence ends with a `fmt: on`, so that the function gets formatted.
#   formatted 1
# fmt: off
    # not formatted
# fmt: on
    # formatted comment
# fmt: off
    # not formatted 2
# fmt: on

    # formatted
def test2  ():
    ...

a =   10

# Sequence of trailing comments that toggles between format on and off. The sequence ends with a `fmt: off`, so that the function is not formatted.
    # formatted 1
# fmt: off
    # not formatted
# fmt: on
    # formattd
# fmt: off

    # not formatted
def test3  ():
    ...

# fmt: on
