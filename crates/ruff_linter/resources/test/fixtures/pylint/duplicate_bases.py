###
# Errors.
###
class A:
    ...


class B:
    ...


# Duplicate base class is last.
class F1(A, A):
    ...


class F2(A, A,):
    ...


class F3(
    A,
    A
):
    ...


class F4(
    A,
    A,
):
    ...


# Duplicate base class is not last.
class G1(A, A, B):
    ...


class G2(A, A, B,):
    ...


class G3(
    A,
    A,
    B
):
    ...


class G4(
    A,
    A,
    B,
):
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
