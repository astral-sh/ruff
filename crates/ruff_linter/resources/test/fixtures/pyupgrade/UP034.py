# UP034
print(("foo"))

# UP034
print(("hell((goodbye))o"))

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

# OK
bases = (object,)


class Foo((base for base in bases)):
    pass


# OK
class Bar(object, (base for base in bases)):
    pass


# OK
class Baz[T]((base for base in bases)):
    pass


# OK
class Multi(
    (base for base in bases)
):
    pass


# UP034
class Qux(f((base for base in bases))):
    pass
