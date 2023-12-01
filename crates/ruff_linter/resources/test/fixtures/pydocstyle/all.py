def public_func():
    pass


def private_func():
    pass


class PublicClass:
    class PublicNestedClass:
        pass


class PrivateClass:
    pass


__all__ = ("public_func", "PublicClass")
