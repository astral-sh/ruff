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

# Comment that could be mistaken for a trailing comment of the function declaration when
# looking from the position of the if
# Regression test for https://github.com/python/cpython/blob/ad56340b665c5d8ac1f318964f71697bba41acb7/Lib/logging/__init__.py#L253-L260
if True:
    def f1():
        pass  # a
else:
    pass

# Here it's actually a trailing comment
if True:
    def f2():
        pass
        # a
else:
    pass

# Make sure the star is printed
# Regression test for https://github.com/python/cpython/blob/7199584ac8632eab57612f595a7162ab8d2ebbc0/Lib/warnings.py#L513
def f(arg1=1, *, kwonlyarg1, kwonlyarg2=2):
    pass


# Regression test for https://github.com/astral-sh/ruff/issues/5176#issuecomment-1598171989
def foo(
    b=3 + 2  # comment
):
    ...


# Comments on the slash or the star, both of which don't have a node
def f11(
    a,
    # positional only comment, leading
    /, # positional only comment, trailing
    b,
):
    pass

def f12(
    a=1,
    # positional only comment, leading
    /, # positional only comment, trailing
    b=2,
):
    pass

def f13(
    a,
    # positional only comment, leading
    /, # positional only comment, trailing
):
    pass

def f21(
    a=1,
    # keyword only comment, leading
    *, # keyword only comment, trailing
    b=2,
):
    pass

def f22(
    a,
    # keyword only comment, leading
    *, # keyword only comment, trailing
    b,
):
    pass

def f23(
    a,
    # keyword only comment, leading
    *args, # keyword only comment, trailing
    b,
):
    pass

def f24(
    # keyword only comment, leading
    *, # keyword only comment, trailing
    a
):
    pass


def f31(
    a=1,
    # positional only comment, leading
    /, # positional only comment, trailing
    b=2,
    # keyword only comment, leading
    *, # keyword only comment, trailing
    c=3,
):
    pass

def f32(
    a,
    # positional only comment, leading
    /, # positional only comment, trailing
    b,
    # keyword only comment, leading
    *, # keyword only comment, trailing
    c,
):
    pass

def f33(
    a,
    # positional only comment, leading
    /,  # positional only comment, trailing
    # keyword only comment, leading
    *args, # keyword only comment, trailing
    c,
):
    pass


def f34(
    a,
    # positional only comment, leading
    /,  # positional only comment, trailing
    # keyword only comment, leading
    *, # keyword only comment, trailing
    c,
):
    pass

def f35(
    # keyword only comment, leading
    *, # keyword only comment, trailing
    c,
):
    pass

# Multiple trailing comments
def f41(
    a,
    / # 1
    , # 2
    # 3
    * # 4
    , # 5
    c,
):
    pass

# Multiple trailing comments strangely places. The goal here is only stable formatting,
# the comments are placed to strangely to keep their relative position intact
def f42(
    a,
    / # 1
    # 2
    , # 3
    # 4
    * # 5
    # 6
    , # 7
    c,
):
    pass
