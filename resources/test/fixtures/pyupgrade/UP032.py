###
# Errors
###

"{} {}".format(a, b)

"{1} {0}".format(a, b)

"{x.y}".format(x=z)

"{.x} {.y}".format(a, b)

"{} {}".format(a.b, c.d)

"{}".format(a())

"{}".format(a.b())

"{}".format(a.b().c())

"hello {}!".format(name)

"{}{b}{}".format(a, c, b=b)

"{}".format(0x0)

"{} {}".format(a, b)

"""{} {}""".format(a, b)

"foo{}".format(1)

r"foo{}".format(1)

x = "{a}".format(a=1)

print("foo {} ".format(x))

"{a[b]}".format(a=a)

"{a.a[b]}".format(a=a)

"{}{{}}{}".format(escaped, y)

"{}".format(a)

'({}={{0!e}})'.format(a)

###
# Non-errors
###

# False-negative: RustPython doesn't parse the `\N{snowman}`.
"\N{snowman} {}".format(a)

"{".format(a)

"}".format(a)

"{} {}".format(*a)

"{0} {0}".format(arg)

"{x} {x}".format(arg)

"{x.y} {x.z}".format(arg)

b"{} {}".format(a, b)

"{:{}}".format(x, y)

"{}{}".format(a)

"" "{}".format(a["\\"])

"{}".format(a["b"])

r'"\N{snowman} {}".format(a)'

"{a}" "{b}".format(a=1, b=1)


async def c():
    return "{}".format(await 3)


async def c():
    return "{}".format(1 + await 3)
