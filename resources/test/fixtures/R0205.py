class A:
    ...


class B(object):
    ...


class C(B, object):
    ...


class D(
    object,
    B,
):
    ...


def f():
    class E(object):
        ...


class F(
    object,
):
    ...


class G(
    object,  # )
):
    ...


object = A


class H(object):
    ...
