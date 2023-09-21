def foo(obj):
    obj._meta  # OK


def foo(obj):
    obj._asdict  # SLF001


def foo(obj):
    obj._bar  # SLF001
