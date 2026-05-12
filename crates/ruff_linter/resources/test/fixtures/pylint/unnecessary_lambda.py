_ = lambda: print()  # [unnecessary-lambda]
_ = lambda x, y: min(x, y) # [unnecessary-lambda]

_ = lambda *args: f(*args) # [unnecessary-lambda]
_ = lambda **kwargs: f(**kwargs) # [unnecessary-lambda]
_ = lambda *args, **kwargs: f(*args, **kwargs) # [unnecessary-lambda]
_ = lambda x, y, z, *args, **kwargs: f(x, y, z, *args, **kwargs) # [unnecessary-lambda]

_ = lambda x: f(lambda x: x)(x) # [unnecessary-lambda]
_ = lambda x, y: f(lambda x, y: x + y)(x, y) # [unnecessary-lambda]

# default value in lambda parameters
_ = lambda x=42: print(x)

# lambda body is not a call
_ = lambda x: x

# ignore chained calls
_ = lambda: chained().call()

# lambda has kwargs but not the call
_ = lambda **kwargs: f()

# lambda has kwargs and the call has kwargs named differently
_ = lambda **kwargs: f(**dict([('forty-two', 42)]))

# lambda has kwargs and the call has unnamed kwargs
_ = lambda **kwargs: f(**{1: 2})

# lambda has starred parameters but not the call
_ = lambda *args: f()

# lambda has starred parameters and the call has starargs named differently
_ = lambda *args: f(*list([3, 4]))
# lambda has starred parameters and the call has unnamed starargs
_ = lambda *args: f(*[3, 4])

# lambda has parameters but not the call
_ = lambda x: f()
_ = lambda *x: f()
_ = lambda **x: f()

# lambda parameters and call args are not the same length
_ = lambda x, y: f(x)

# lambda parameters and call args are not in the same order
_ = lambda x, y: f(y, x)

# lambda parameters and call args are not the same
_ = lambda x: f(y)

# the call uses the lambda parameters
_ = lambda x: x(x)
_ = lambda x, y: x(x, y)
_ = lambda x: z(lambda y: x + y)(x)

# lambda uses an additional keyword
_ = lambda *args: f(*args, y=1)
_ = lambda *args: f(*args, y=x)

# https://github.com/astral-sh/ruff/issues/18675
_ = lambda x: (string := str)(x)
_ = lambda x: ((x := 1) and str)(x)

# https://github.com/astral-sh/ruff/issues/24704
# Forward references: the callable is defined after the lambda, so the
# lambda's late binding is the only thing keeping the code working —
# inlining would raise `NameError`.
_ = lambda y: forward(y)
_ = {"a": lambda y: forward(y)}


def forward(y):
    pass


# Backward references in the same module are still safe to inline.
def already_defined(y):
    pass


_ = lambda y: already_defined(y) # [unnecessary-lambda]


# https://github.com/astral-sh/ruff/pull/25074#pullrequestreview-4272365684
# Self ref via assignment RHS: `g` isn't bound when the lambda evaluates.
g = lambda y: g(y)


# Self ref via def default arg: same shape, in a parameter default.
def later(x=lambda y: later(y)):
    return x


# Lazy self ref: lambda only evaluates when `lazy_self` is called, by
# which point the binding has resolved — still safe to inline.
def lazy_self():
    return lambda y: lazy_self(y) # [unnecessary-lambda]
