"""
Should emit:
B009 - Line 18, 19, 20, 21, 22
B010 - Line 33, 34, 35, 36
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

# Invalid usage
getattr(foo, "bar")
getattr(foo, "_123abc")
getattr(foo, "abc123")
getattr(foo, r"abc123")
_ = lambda x: getattr(x, "bar")
if getattr(x, "bar"):
    pass

# Valid setattr usage
setattr(foo, bar, None)
setattr(foo, "bar{foo}".format(foo="a"), None)
setattr(foo, "123abc", None)
setattr(foo, r"123\abc", None)
setattr(foo, "except", None)
_ = lambda x: setattr(x, "bar", 1)
if setattr(x, "bar", 1):
    pass

# Invalid usage
setattr(foo, "bar", None)
setattr(foo, "_123abc", None)
setattr(foo, "abc123", None)
setattr(foo, r"abc123", None)
setattr(foo.bar, r"baz", None)
