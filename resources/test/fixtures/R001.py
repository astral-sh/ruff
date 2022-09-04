class A:
    ...


class A(object):
    ...


class A(
    object,
):
    ...


class A(
    object,
    #
):
    ...


class A(
    #
    object,
):
    ...


class A(
    #
    object
):
    ...


class A(
    object
    #
):
    ...


class A(
    #
    object,
    #
):
    ...


class A(
    #
    object,
    #
):
    ...


class A(
    #
    object
    #
):
    ...


class A(
    #
    object
    #
):
    ...


class B(A, object):
    ...


class B(object, A):
    ...


class B(
    object,
    A,
):
    ...


class B(
    A,
    object,
):
    ...


class B(
    object,
    # Comment on A.
    A,
):
    ...


class B(
    # Comment on A.
    A,
    object,
):
    ...


def f():
    class A(object):
        ...


class A(
    object,
):
    ...


class A(
    object,  # )
):
    ...


class A(
    object  # )
    ,
):
    ...


object = A


class B(object):
    ...
