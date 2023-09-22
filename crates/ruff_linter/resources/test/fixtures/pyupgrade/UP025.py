# These should change
x = u"Hello"

u'world'

print(u"Hello")

print(u'world')

import foo

foo(u"Hello", U"world", a=u"Hello", b=u"world")

# These should stay quoted they way they are

x = u'hello'
x = u"""hello"""
x = u'''hello'''
x = u'Hello "World"'

# These should not change
u = "Hello"

u = u

def hello():
    return"Hello"
