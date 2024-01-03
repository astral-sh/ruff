u"Hello"

x = u"Hello"  # UP025

u'world'  # UP025

print(u"Hello")  # UP025

print(u'world')  # UP025

import foo

foo(u"Hello", U"world", a=u"Hello", b=u"world")  # UP025

# Retain quotes when fixing.
x = u'hello'  # UP025
x = u"""hello"""  # UP025
x = u'''hello'''  # UP025
x = u'Hello "World"'  # UP025

u = "Hello"  # OK
u = u  # OK

def hello():
    return"Hello"  # OK

f"foo"u"bar"  # OK
f"foo" u"bar"  # OK
