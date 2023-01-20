# UP034
print(("foo"))

# UP034
print(("hell((goodybe))o"))

# UP034
print((("foo")))

# UP034
print((((1))))

# UP034
print(("foo{}".format(1)))

# UP034
print(
    ("foo{}".format(1))
)

# UP034
print(
    (
        "foo"
    )
)

# UP034
def f():
    x = int(((yield 1)))

# UP034
if True:
    print(
        ("foo{}".format(1))
    )

# UP034
print((x for x in range(3)))

# OK
print("foo")

# OK
print((1, 2, 3))

# OK
print(())

# OK
print((1,))

# OK
sum((block.code for block in blocks), [])

# OK
def f():
    x = int((yield 1))

# OK
sum((i for i in range(3)), [])
