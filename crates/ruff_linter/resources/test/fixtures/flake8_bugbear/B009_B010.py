"""
Should emit:
B009 - Lines 19-31
B010 - Lines 40-45
"""

# Valid getattr usage
getattr(foo, bar)
getattr(foo, "bar", None)
getattr(foo, "bar{foo}".format(foo="a"), None)
getattr(foo, "bar{foo}".format(foo="a"))
getattr(foo, bar, None)
getattr(foo, "123abc")
getattr(foo, r"123\abc")
getattr(foo, "except")
getattr(foo, "__123abc")

# Invalid usage
getattr(foo, "bar")
getattr(foo, "_123abc")
getattr(foo, "__123abc__")
getattr(foo, "abc123")
getattr(foo, r"abc123")
_ = lambda x: getattr(x, "bar")
if getattr(x, "bar"):
    pass
getattr(1, "real")
getattr(1., "real")
getattr(1.0, "real")
getattr(1j, "real")
getattr(True, "real")
getattr(x := 1, "real")
getattr(x + y, "real")
getattr("foo"
        "bar", "real")


# Valid setattr usage
setattr(foo, bar, None)
setattr(foo, "bar{foo}".format(foo="a"), None)
setattr(foo, "123abc", None)
setattr(foo, "__123abc", None)
setattr(foo, r"123\abc", None)
setattr(foo, "except", None)
_ = lambda x: setattr(x, "bar", 1)
if setattr(x, "bar", 1):
    pass

# Invalid usage
setattr(foo, "bar", None)
setattr(foo, "_123abc", None)
setattr(foo, "__123abc__", None)
setattr(foo, "abc123", None)
setattr(foo, r"abc123", None)
setattr(foo.bar, r"baz", None)

# Regression test for: https://github.com/astral-sh/ruff/issues/7455#issuecomment-1722458885
assert getattr(func, '_rpc')is True

# Regression test for: https://github.com/astral-sh/ruff/issues/7455#issuecomment-1732387247
getattr(*foo, "bar")
setattr(*foo, "bar", None)

# Regression test for: https://github.com/astral-sh/ruff/issues/7455#issuecomment-1739800901
getattr(self.
   registration.registry, '__name__')

import builtins
builtins.getattr(foo, "bar")
