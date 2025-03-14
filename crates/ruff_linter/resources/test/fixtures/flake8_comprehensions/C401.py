# Cannot combine with C416. Should use set comprehension here.
even_nums = set(2 * x for x in range(3))
odd_nums = set(
    2 * x + 1 for x in range(3)
)
small_nums = f"{set(a if a < 6 else 0 for a in range(3))}"

def f(x):
    return x

print(f"Hello {set(f(a) for a in 'abc')} World")
print(f"Hello { set(f(a) for a in 'abc') } World")


# Short-circuit case, combine with C416 and should produce x = set(range(3))
x = set(x for x in range(3))
x = set(
    x for x in range(3)
)
print(f"Hello {set(a for a in range(3))} World")
print(f"{set(a for a in 'abc') - set(a for a in 'ab')}")
print(f"{ set(a for a in 'abc') - set(a for a in 'ab') }")

# Strip parentheses from inner generators.
set((2 * x for x in range(3)))
set(((2 * x for x in range(3))))
set((((2 * x for x in range(3)))))

# Account for trailing comma in fix
# See https://github.com/astral-sh/ruff/issues/15852
set((0 for _ in []),)
set(
    (0 for _ in [])
    # some comments
    ,
    # some more
)

# Not built-in set.
def set(*args, **kwargs):
    return None

set(2 * x for x in range(3))
set(x for x in range(3))
