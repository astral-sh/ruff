# Dangling comments
def test(
    # comment

    # another

): ...


# Argument empty line spacing
def test(
    # comment
    a,

    # another

    b,
): ...


### Different function argument wrappings

def single_line(aaaaaaaaaaaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbb, ccccccccccccccccc):
    pass

def arguments_on_their_own_line(aaaaaaaaaaaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbb, ccccccccccc, ddddddddddddd, eeeeeee):
    pass

def argument_per_line(aaaaaaaaaaaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbb, ccccccccccccccccc, ddddddddddddd, eeeeeeeeeeeeeeee, ffffffffffff):
    pass

def last_pos_only_trailing_comma(a, b, /,):
    pass

def last_pos_no_trailing_comma(a, b, /):
    pass


def varg_with_leading_comments(
    a, b,
    # comment
    *args
): ...

def kwarg_with_leading_comments(
    a, b,
    # comment
    **kwargs
): ...

def argument_with_long_default(
    a,
    b = ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc + [
        dddddddddddddddddddd, eeeeeeeeeeeeeeeeeeee, ffffffffffffffffffffffff
    ],
    h = []
):  ...


def argument_with_long_type_annotation(
    a,
    b: xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx | yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy | zzzzzzzzzzzzzzzzzzz = [0, 1, 2, 3],
    h = []
):  ...


def test(): ...

# Comment
def with_leading_comment(): ...
