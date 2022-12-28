def f(*args, **kwargs):
    pass


a = (1, 2)
b = (3, 4)
c = (5, 6)

f(a, b)
f(a, kw=b)
f(*a, kw=b)
f(kw=a, *b)
f(kw=a, *b, *c)
