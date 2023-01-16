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

u"foo{}".format(1)

# Waiting for a response to solve this one
# "{}{{}}{}".format(escaped, y)

# Don't forget assigned, call, and multiline
# Waiting on Charlie solution to solve this
# r'"\N{snowman} {}".format(a)'
