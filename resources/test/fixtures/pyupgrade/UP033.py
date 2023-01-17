# These SHOULD work
print(("foo"))

print(("hell((goodybe))o"))

print((("foo")))

# These SHOULD NOT work
print("foo")

print(())

print((1,))

sum((i for i in range(3)), [])

print((x for x in range(3)))

print ((
    "foo"
))

print(
    ("foo")
)
