###
# Blank lines around functions
###
import sys

x = 1

# comment

def f():
    pass


if True:
    x = 1

# comment

def f():
    pass


x = 1



# comment

def f():
    pass


x = 1



# comment
def f():
    pass


x = 1

# comment

# comment
def f():
    pass

x = 1

# comment
# comment

def f():
    pass

x = 1

# comment
# comment
def f():
    pass


x = 1


# comment



# comment



def f():
    pass
# comment


def f():
    pass

# comment

def f():
    pass


# comment

###
# Blank lines around imports.
###

def f():
    import x
    # comment
    import y


def f():
    import x

    # comment
    import y


def f():
    import x
    # comment

    import y


def f():
    import x
    # comment


    import y


def f():
    import x


    # comment
    import y


def f():
    import x

    # comment

    import y


def f():
    import x  # comment
    # comment

    import y


def f(): pass  # comment
# comment

x = 1


def f():
    pass




# comment

x = 1


def f():
    if True:

        def double(s):
            return s + s
    print("below function")
    if True:

        class A:
            x = 1
    print("below class")
    if True:

        def double(s):
            return s + s
        #
    print("below comment function")
    if True:

        class A:
            x = 1
        #
    print("below comment class")
    if True:

        def double(s):
            return s + s
    #
    print("below comment function 2")
    if True:

        def double(s):
            return s + s
            #
    def outer():
        def inner():
            pass
    print("below nested functions")

if True:

    def double(s):
        return s + s
print("below function")
if True:

    class A:
        x = 1
print("below class")
def outer():
    def inner():
        pass
print("below nested functions")


class Path:
    if sys.version_info >= (3, 11):
        def joinpath(self): ...

    # The .open method comes from pathlib.pyi and should be kept in sync.
    @overload
    def open(self): ...




def fakehttp():

    class FakeHTTPConnection:
        if mock_close:
            def close(self):
                pass
    FakeHTTPConnection.fakedata = fakedata





if True:
    if False:
        def x():
            def y():
                pass
        #comment
    print()


# NOTE: Please keep this the last block in this file. This tests that we don't insert
# empty line(s) at the end of the file due to nested function
if True:
    def nested_trailing_function():
        pass