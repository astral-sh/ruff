foo: object

# Errors.

if isinstance(foo, type(None)):
    pass

if isinstance(foo and bar, type(None)):
    pass

if isinstance(foo, (type(None), type(None), type(None))):
    pass

if isinstance(foo, type(None)) is True:
    pass

if -isinstance(foo, type(None)):
    pass

if isinstance(foo, None | type(None)):
    pass

if isinstance(foo, type(None) | type(None)):
    pass

# A bit contrived, but is both technically valid and equivalent to the above.
if isinstance(foo, (type(None) | ((((type(None))))) | ((None | type(None))))):
    pass

if isinstance(
    foo,  # Comment
    None
):
    ...

from typing import Union

if isinstance(foo, Union[None]):
    ...

if isinstance(foo, Union[None, None]):
    ...

if isinstance(foo, Union[None, type(None)]):
    ...


# Okay.

if isinstance(foo, int):
    pass

if isinstance(foo, (int)):
    pass

if isinstance(foo, (int, str)):
    pass

if isinstance(foo, (int, type(None), str)):
    pass

if isinstance(foo, str | None):
    pass

if isinstance(foo, Union[None, str]):
    ...

# This is a TypeError, which the rule ignores.
if isinstance(foo, None):
    pass

# This is also a TypeError, which the rule ignores.
if isinstance(foo, (None,)):
    pass

if isinstance(foo, None | None):
    pass

if isinstance(foo, (type(None) | ((((type(None))))) | ((None | None | type(None))))):
    pass

# https://github.com/astral-sh/ruff/issues/15776
def _():
    def type(*args): ...

    if isinstance(foo, type(None)):
        ...
