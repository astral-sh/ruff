lambda: a
lambda: 1
lambda x: 1
lambda x, y: ...
lambda a, b, c: 1
lambda a, b=20, c=30: 1
lambda x, y: x * y
lambda y, z=1: z * y
lambda *a: a
lambda *a, z, x=0: ...
lambda *, a, b, c: 1
lambda *, a, b=20, c=30: 1
lambda a, b, c, *, d, e: 0
lambda **kwargs: f()
lambda *args, **kwargs: f() + 1
lambda *args, a, b=1, **kwargs: f() + 1
lambda a, /: ...
lambda a, /, b: ...
lambda a=1, /,: ...
lambda a, b, /, *, c: ...
lambda kw=1, *, a: ...
lambda a, b=20, /, c=30: 1
lambda a, b, /, c, *, d, e: 0
lambda a, b, /, c, *d, e, **f: 0