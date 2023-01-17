# These SHOULD change
f"{a} {b}"

"{1} {0}".format(a, b)

f"{z.y}"

f"{a.x} {b.y}"

f"{a.b} {c.d}"

f"{a()}"

f"{a.b()}"

f"{a.b().c()}"

f"hello {name}!"

f"{a}{b}{c}"

f"{0x0}"

f"foo{1}"

x = f"{1}"

print(f"foo {x} ")

# Waiting for a response to solve this one
# "{}{{}}{}".format(escaped, y)

# Waiting on Charlie solution to solve this
# r'"\N{snowman} {}".format(a)'

# These should NOT change

"{".format(a)

"}".format(a)

"{}".format(
    a
)

"{} {}".format(*a)

"{0} {0}".format(arg)

"{x} {x}".format(arg)

"{x.y} {x.z}".format(arg)

b"{} {}".format(a, b)

"{:{}}".format(x, y)

"{a[b]}".format(a=a)

"{a.a[b]}".format(a=a)

"{}{}".format(a)

''"{}".format(a['\\'])

'{}'.format(a['b'])

async def c(): return '{}'.format(await 3)

async def c(): return '{}'.format(1 + await 3)
