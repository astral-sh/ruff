# These SHOULD change
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

'{} {}'.format(a, b)

'''{} {}'''.format(a, b)

u"foo{}".format(1)

r"foo{}".format(1)

x = "{a}".format(a=1)

print("foo {} ".format(x))

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
