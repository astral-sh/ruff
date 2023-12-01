# Tricky sequences of fmt off and on

# Formatted
a +   b

# fmt: off
    # not formatted 1
# fmt: on
a   + b
    # formatted


# fmt: off
    # not formatted 1
# fmt: on
    # not formatted 2
# fmt: off
a   + b
# fmt: on


# fmt: off
    # not formatted 1
# fmt: on
    # formatted 1
# fmt: off
    # not formatted 2
a   + b
# fmt: on
    # formatted
b   + c


# fmt: off
a   + b

    # not formatted
# fmt: on
    # formatted
a   + b


# fmt: off
a   + b

    # not formatted 1
# fmt: on
    # formatted
# fmt: off
    # not formatted 2
a   + b


# fmt: off
a   + b

    # not formatted 1
# fmt: on
    # formatted

# leading
a    + b
# fmt: off

    # leading unformatted
def test  ():
    pass

 # fmt: on

a   + b
