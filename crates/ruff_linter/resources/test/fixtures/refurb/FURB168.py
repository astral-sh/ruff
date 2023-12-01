foo: object

# Errors.

if isinstance(foo, type(None)):
    pass

if isinstance(foo, (type(None))):
    pass

if isinstance(foo, (type(None), type(None), type(None))):
    pass

if isinstance(foo, None | None):
    pass

if isinstance(foo, (None | None)):
    pass

if isinstance(foo, None | type(None)):
    pass

if isinstance(foo, (None | type(None))):
    pass

# A bit contrived, but is both technically valid and equivalent to the above.
if isinstance(foo, (type(None) | ((((type(None))))) | ((None | type(None))))):
    pass


# Okay.

if isinstance(foo, int):
    pass

if isinstance(foo, (int)):
    pass

if isinstance(foo, (int, str)):
    pass

if isinstance(foo, (int, type(None), str)):
    pass

# This is a TypeError, which the rule ignores.
if isinstance(foo, None):
    pass

# This is also a TypeError, which the rule ignores.
if isinstance(foo, (None,)):
    pass
