def foo(fun):
    def wrapper():
        print('before')
        fun()
        print('after')
    return wrapper

# No error
@foo
def bar():
    print('bar')

# E204
@ foo
def baz():
    print('baz')

class Test:
    # No error
    @foo
    def bar(self):
        print('bar')

    # E204
    @ foo
    def baz(self):
        print('baz')


# E204
@ \
foo
def baz():
    print('baz')
