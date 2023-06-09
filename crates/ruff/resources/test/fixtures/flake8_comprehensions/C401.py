x = set(x for x in range(3))
x = set(x for x in range(3))
y = f"{set(a if a < 6 else 0  for a in range(3))}"
_ = "{}".format(set(a if a < 6 else 0 for a in range(3)))
print(f"Hello {set(a for a in range(3))} World")


def f(x):
    return x


print(f'Hello {set(a for a in "abc")} World')
print(f"Hello {set(a for a in 'abc')} World")
print(f"Hello {set(f(a) for a in 'abc')} World")
print(f"{set(a for a in 'abc') - set(a for a in 'ab')}")
print(f"{ set(a for a in 'abc') - set(a for a in 'ab') }")

# The fix generated for this diagnostic is incorrect, as we add additional space
# around the set comprehension.
print(f"{ {set(a for a in 'abc')} }")
