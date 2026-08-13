# Errors.

if 1 in (1,):
    print("Single-element tuple")

if 1 in [1]:
    print("Single-element list")

if 1 in {1}:
    print("Single-element set")

if "a" in "a":
    print("Single-element string")

if 1 not in (1,):
    print("Check `not in` membership test")

if not 1 in (1,):
    print("Check the negated membership test")

# Non-errors.

if 1 in (1, 2):
    pass

if 1 in [1, 2]:
    pass

if 1 in {1, 2}:
    pass

if "a" in "ab":
    pass

if 1 == (1,):
    pass

if 1 > [1]:
    pass

if 1 is {1}:
    pass

if "a" == "a":
    pass

if 1 in {*[1]}:
    pass


# https://github.com/astral-sh/ruff/issues/10063
_ = a in (
    # Foo
    b,
)

_ = a in (  # Foo1
    (  # Foo2
    # Foo3
        (  # Tuple
            (  # Bar
       (b
        # Bar
        )
  )
       # Foo4
            # Foo5
     ,
       )
        # Foo6
    )
)

foo = (
    lorem()
        .ipsum()
        .dolor(lambda sit: sit in (
            # Foo1
            # Foo2
            amet,
        ))
)

foo = (
    lorem()
        .ipsum()
        .dolor(lambda sit: sit in (
            (
                # Foo1
                # Foo2
                amet
            ),
        ))
)

foo = lorem() \
    .ipsum() \
    .dolor(lambda sit: sit in (
        # Foo1
        # Foo2
        amet,
    ))

def _():
    if foo not \
        in [
        # Before
        bar
        # After
    ]: ...

def _():
    if foo not \
        in [
        # Before
        bar
        # After
    ] and \
        0 < 1: ...

# https://github.com/astral-sh/ruff/issues/20255
import math

# NaN behavior differences
if math.nan in [math.nan]:
    print("This is True")

if math.nan in (math.nan,):
    print("This is True")

if math.nan in {math.nan}:
    print("This is True")

# Potential type differences with custom __eq__ methods
class CustomEq:
    def __eq__(self, other):
        return "custom"

obj = CustomEq()
if obj in [CustomEq()]:
    pass

if obj in (CustomEq(),):
    pass

if obj in {CustomEq()}:
    pass
