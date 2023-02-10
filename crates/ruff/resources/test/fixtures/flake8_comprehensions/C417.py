nums = [1, 2, 3]
map(lambda x: x + 1, nums)
map(lambda x: str(x), nums)
list(map(lambda x: x * 2, nums))
set(map(lambda x: x % 2 == 0, nums))
dict(map(lambda v: (v, v**2), nums))
map(lambda: "const", nums)
map(lambda _: 3.0, nums)

# valid, but no autofix
# for simple expressions this could be `(x if x else 1 for x in nums)`
# for more complex expressions this would be different e.g. `(x + 2 if x else 3 for x in nums)`
map(lambda x=1: x, nums)

# valid, but not matched by C417 currently
map(lambda x=2, y=1: x + y, nums, nums)
set(map(lambda x, y: x, nums, nums))

# valid, but out of scope for C417 in flake8-comprehensions
def myfunc(arg1: int, arg2: int = 4):
    return 2 * arg1 + arg2

list(map(myfunc, nums))


[x for x in nums]
