# These SHOULD work
print(("foo"))

print(("hell((goodybe))o"))

print((("foo")))

print((((1))))

print(("foo{}".format(1)))

print(
    ("foo{}".format(1))
)

print(
    (
        "foo"
    )
)

def f():
    x = int(((yield 1)))

# These SHOULD NOT work
print("foo")

print((1, 2, 3))

print(())

print((1,))

sum((block.code for block in blocks), [])

def f():
    x = int((yield 1))

sum((i for i in range(3)), [])

print((x for x in range(3)))

print ((
    "foo"
))

print(
    ("foo")
)
