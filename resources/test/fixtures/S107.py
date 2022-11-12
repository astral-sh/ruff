def ok(first, default="default"):
    pass


def default(first, password="default"):
    pass


def ok_posonly(first, /, pos, default="posonly"):
    pass


def default_posonly(first, /, pos, password="posonly"):
    pass


def ok_kwonly(first, *, default="kwonly"):
    pass


def default_kwonly(first, *, password="kwonly"):
    pass


def ok_all(first, /, pos, default="posonly", *, kwonly="kwonly"):
    pass


def default_all(first, /, pos, secret="posonly", *, password="kwonly"):
    pass
