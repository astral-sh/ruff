(
    "First entry",
    "Second entry",
    "last with trailing comma",
)

(
    "First entry",
    "Second entry",
    "last without trailing comma"
)

(
    "First entry",
    "Second entry",
    "third entry",
    "fourth entry",
    "fifth entry",
    "sixt entry",
    "seventh entry",
    "eighth entry",
)

# Regression test: Respect setting in Arguments formatting
def f(a): pass
def g(a,): pass

x1 = lambda y: 1
x2 = lambda y,: 1

# Ignore trailing comma.
with (a,):  # magic trailing comma
    ...
