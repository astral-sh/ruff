# Errors.
nums = [1, 2, 3]
map(lambda x: x + 1, nums)
map(lambda x: str(x), nums)
list(map(lambda x: x * 2, nums))
set(map(lambda x: x % 2 == 0, nums))
dict(map(lambda v: (v, v**2), nums))
dict(map(lambda v: [v, v**2], nums))
map(lambda: "const", nums)
map(lambda _: 3.0, nums)
_ = "".join(map(lambda x: x in nums and "1" or "0", range(123)))
all(map(lambda v: isinstance(v, dict), nums))
filter(func, map(lambda v: v, nums))
list(map(lambda x, y: x * y, nums))

# When inside f-string, then the fix should be surrounded by whitespace
_ = f"{set(map(lambda x: x % 2 == 0, nums))}"
_ = f"{dict(map(lambda v: (v, v**2), nums))}"

# False negatives.
map(lambda x=2, y=1: x + y, nums, nums)
set(map(lambda x, y: x, nums, nums))


def func(arg1: int, arg2: int = 4):
    return 2 * arg1 + arg2


# Non-error: `func` is not a lambda.
list(map(func, nums))

# False positive: need to preserve the late-binding of `x` in the inner lambda.
map(lambda x: lambda: x, range(4))

# Error: the `x` is overridden by the inner lambda.
map(lambda x: lambda x: x, range(4))

# Ok because of the default parameters, and variadic arguments.
map(lambda x=1: x, nums)
map(lambda *args: len(args), range(4))
map(lambda **kwargs: len(kwargs), range(4))

# Ok because multiple arguments are allowed.
dict(map(lambda k, v: (k, v), keys, values))

# Regression test for: https://github.com/astral-sh/ruff/issues/7121
map(lambda x: x, y if y else z)
map(lambda x: x, (y if y else z))
map(lambda x: x, (x, y, z))
