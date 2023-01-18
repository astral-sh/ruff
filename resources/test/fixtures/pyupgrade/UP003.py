type("")
type(b"")
type(0)
type(0.0)
type(0j)

# OK
y = x.dtype.type(0.0)

# OK
type = lambda *args, **kwargs: None
type("")
