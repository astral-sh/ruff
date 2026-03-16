"""
Should emit:
B043 - Lines 16-20
"""

# Valid delattr usage
delattr(foo, bar)
delattr(foo, "bar{foo}".format(foo="a"))
delattr(foo, "123abc")
delattr(foo, "__123abc")
delattr(foo, r"123\abc")
delattr(foo, "except")
_ = lambda x: delattr(x, "bar")
if delattr(x, "bar"):
    pass

# Invalid usage
delattr(foo, "bar")
delattr(foo, "_123abc")
delattr(foo, "__123abc__")
delattr(foo, "abc123")
delattr(foo, r"abc123")

# Starred argument
delattr(*foo, "bar")

# Non-NFKC attribute name (unsafe fix)
delattr(foo, "\u017f")

# Comment in expression (unsafe fix)
delattr(
    obj,
    # text
    "foo",
)

import builtins
builtins.delattr(foo, "bar")
