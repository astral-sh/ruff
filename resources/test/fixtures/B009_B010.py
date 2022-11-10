"""
Should emit:
B009 - Line 17, 18, 19, 44
B010 - Line 28, 29, 30
"""

# Valid getattr usage
getattr(foo, bar)
getattr(foo, "bar", None)
getattr(foo, "bar{foo}".format(foo="a"), None)
getattr(foo, "bar{foo}".format(foo="a"))
getattr(foo, bar, None)
getattr(foo, "123abc")
getattr(foo, "except")

# Invalid usage
getattr(foo, "bar")
getattr(foo, "_123abc")
getattr(foo, "abc123")

# Valid setattr usage
setattr(foo, bar, None)
setattr(foo, "bar{foo}".format(foo="a"), None)
setattr(foo, "123abc", None)
setattr(foo, "except", None)

# Invalid usage
setattr(foo, "bar", None)
setattr(foo, "_123abc", None)
setattr(foo, "abc123", None)

# Allow use of setattr within lambda expression
# since assignment is not valid in this context.
c = lambda x: setattr(x, "some_attr", 1)


class FakeCookieStore:
    def __init__(self, has_setter):
        self.cookie_filter = None
        if has_setter:
            self.setCookieFilter = lambda func: setattr(self, "cookie_filter", func)


# getattr is still flagged within lambda though
c = lambda x: getattr(x, "some_attr")
# should be replaced with
c = lambda x: x.some_attr
