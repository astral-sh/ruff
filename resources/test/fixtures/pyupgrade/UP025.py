# These should change
x = u"Hello"

u'world'

print(u"Hello")

print(u'world')

import foo

foo(u"Hello", u"world", a=u"Hello", b=u"world")

# These should not change
u = "Hello"

u = u

def hello():
    return"Hello"
