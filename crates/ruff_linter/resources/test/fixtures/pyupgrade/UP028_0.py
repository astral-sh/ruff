def f():
    for x in y:
        yield x


def g():
    for x, y in z:
        yield (x, y)


def h():
    for x in [1, 2, 3]:
        yield x


def i():
    for x in {x for x in y}:
        yield x


def j():
    for x in (1, 2, 3):
        yield x


def k():
    for x, y in {3: "x", 6: "y"}:
        yield x, y


def f():  # Comment one\n'
    # Comment two\n'
    for x, y in {  # Comment three\n'
        3: "x",  # Comment four\n'
        # Comment five\n'
        6: "y",  # Comment six\n'
    }:  # Comment seven\n'
        # Comment eight\n'
        yield x, y  # Comment nine\n'
        # Comment ten',


def f():
    for x, y in [{3: (3, [44, "long ss"]), 6: "y"}]:
        yield x, y


def f():
    for x, y in z():
        yield x, y

def f():
    def func():
        # This comment is preserved\n'
        for x, y in z():  # Comment one\n'
            # Comment two\n'
            yield x, y  # Comment three\n'
            # Comment four\n'
# Comment\n'
def g():
    print(3)


def f():
    for x in y:
        yield x
    for z in x:
        yield z


def f():
    for x, y in z():
        yield x, y
    x = 1


# Regression test for: https://github.com/astral-sh/ruff/issues/7103
def _serve_method(fn):
    for h in (
        TaggedText.from_file(args.input)
            .markup(highlight=args.region)
    ):
        yield h


# UP028: The later loop variable is not a reference to the earlier loop variable
def f():
    for x in (1, 2, 3):
        yield x
    # Shadowing with another loop
    for x in (1, 2, 3):
        yield x


# UP028: The exception binding is not a reference to the loop variable
def f():
    for x in (1, 2, 3):
        yield x
    # Shadowing with an `except`
    try:
        pass
    except Exception as x:
        pass


# UP028: The context binding is not a reference to the loop variable
def f():
    for x in (1, 2, 3):
        yield x
    # Shadowing with `with`
    with contextlib.nullcontext() as x:
        pass



# UP028: The type annotation binding is not a reference to the loop variable
def f():
    for x in (1, 2, 3):
        yield x
    # Shadowing with a type annotation
    x: int


# OK: The `del` statement requires the loop variable to exist
def f():
    for x in (1, 2, 3):
        yield x
    # Shadowing with `del`
    del x


# UP028: The exception bindings are not a reference to the loop variable
def f():
    for x in (1, 2, 3):
        yield x
    # Shadowing with multiple `except` blocks
    try:
        pass
    except Exception as x:
        pass
    try:
        pass
    except Exception as x:
        pass


# OK: The `del` statement requires the loop variable to exist
def f():
    for x in (1, 2, 3):
        yield x
    # Shadowing with multiple `del` statements
    del x
    del x


# OK: The `print` call requires the loop variable to exist
def f():
    for x in (1, 2, 3):
        yield x
    # Shadowing with a reference and non-reference binding
    print(x)
    try:
        pass
    except Exception as x:
        pass


# https://github.com/astral-sh/ruff/issues/15540
def f():
    for a in 1,:
        yield a


SOME_GLOBAL = None

def f(iterable):
    global SOME_GLOBAL

    for SOME_GLOBAL in iterable:
        yield SOME_GLOBAL

    some_non_local = None

    def g():
        nonlocal some_non_local

        for some_non_local in iterable:
            yield some_non_local
