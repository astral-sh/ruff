from itertools import batched, count, cycle, repeat


# Errors
batched(range(3), 1)
batched("abc", 2)
batched([i for i in range(42)], some_n)
batched((foo for foo in cycle()))
batched(itertools.batched([1, 2, 3], strict=True))

# Errors (limited iterators).
batched(repeat(1, 1))
batched(repeat(1, times=4))

# No fix
batched([], **kwargs)

# No errors
batched()
batched(range(3), 0, strict=True)
batched(["a", "b"], count, strict=False)
batched(("a", "b", "c"), zip(repeat()), strict=True)

# No errors (infinite iterators)
batched(cycle("ABCDEF"), 3)
batched(count(), qux + lorem)
batched(repeat(1), ipsum // 19 @ 0x1)
batched(repeat(1, None))
batched(repeat(1, times=None))


import itertools

# Errors
itertools.batched(range(3), 1)
itertools.batched("abc", 2)
itertools.batched([i for i in range(42)], some_n)
itertools.batched((foo for foo in cycle()))
itertools.batched(itertools.batched([1, 2, 3], strict=True))

# Errors (limited iterators).
itertools.batched(repeat(1, 1))
itertools.batched(repeat(1, times=4))

# No fix
itertools.batched([], **kwargs)

# No errors
itertools.batched()
itertools.batched(range(3), 0, strict=True)
itertools.batched(["a", "b"], count, strict=False)
itertools.batched(("a", "b", "c"), zip(repeat()), strict=True)

# No errors (infinite iterators)
itertools.batched(cycle("ABCDEF"), 3)
itertools.batched(count(), qux + lorem)
itertools.batched(repeat(1), ipsum // 19 @ 0x1)
itertools.batched(repeat(1, None))
itertools.batched(repeat(1, times=None))
