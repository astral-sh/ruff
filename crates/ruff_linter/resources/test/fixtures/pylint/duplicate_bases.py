###
# Errors.
###
class A:
    ...


class B(A, A):
    ...


###
# Non-errors.
###
class C:
    ...


class D(C):
    ...


class E(A, C):
    ...
