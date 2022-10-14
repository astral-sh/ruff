nums = [1, 2, 3]
map(lambda x: x + 1, nums)
map(lambda x: str(x), nums)
list(map(lambda x: x * 2, nums))
set(map(lambda x: x % 2 == 0, nums))
dict(map(lambda v: (v, v**2), nums))
