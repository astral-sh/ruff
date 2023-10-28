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
