"""foo"""  # OK, not in stub


def foo():
    """foo"""  # OK, doc strings are allowed in non-stubs


class Bar:
    """bar"""  # OK, doc strings are allowed in non-stubs
