# Errors
class A():
    pass


class A() \
    :
    pass


class A \
        ():
    pass


@decorator()
class A():
    pass

@decorator
class A():
    pass

# OK
class A:
    pass


class A(A):
    pass


class A(metaclass=type):
    pass


@decorator()
class A:
    pass

@decorator
class A:
    pass
