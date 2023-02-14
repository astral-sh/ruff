# Errors.
nums = [1, 2, 3]
map(lambda x: x + 1, nums)
map(lambda x: str(x), nums)
list(map(lambda x: x * 2, nums))
set(map(lambda x: x % 2 == 0, nums))
dict(map(lambda v: (v, v**2), nums))
map(lambda: "const", nums)
map(lambda _: 3.0, nums)
_ = "".join(map(lambda x: x in nums and "1" or "0", range(123)))
all(map(lambda v: isinstance(v, dict), nums))
filter(func, map(lambda v: v, nums))

# When inside f-string, then the fix should be surrounded by whitespace
_ = f"{set(map(lambda x: x % 2 == 0, nums))}"
_ = f"{dict(map(lambda v: (v, v**2), nums))}"

# Error, but unfixable.
# For simple expressions, this could be: `(x if x else 1 for x in nums)`.
# For more complex expressions, this would differ: `(x + 2 if x else 3 for x in nums)`.
map(lambda x=1: x, nums)

# False negatives.
map(lambda x=2, y=1: x + y, nums, nums)
set(map(lambda x, y: x, nums, nums))


def myfunc(arg1: int, arg2: int = 4):
    return 2 * arg1 + arg2


list(map(myfunc, nums))

[x for x in nums]
