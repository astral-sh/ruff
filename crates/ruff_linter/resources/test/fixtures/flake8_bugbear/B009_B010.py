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

# Regression test for: https://github.com/astral-sh/ruff/issues/18353
setattr(foo, "__debug__", 0)

# Regression test for: https://github.com/astral-sh/ruff/issues/21126
# Non-NFKC attribute names should be marked as unsafe. Python normalizes identifiers in
# attribute access (obj.attr) using NFKC, but does not normalize string
# arguments passed to getattr/setattr. Rewriting `getattr(ns, "ſ")` to
# `ns.ſ` would be interpreted as `ns.s` at runtime, changing behavior.
# Example: the long s character "ſ" normalizes to "s" under NFKC.
getattr(foo, "ſ")
setattr(foo, "ſ", 1)
