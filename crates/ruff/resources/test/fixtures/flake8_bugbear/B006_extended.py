import custom
from custom import ImmutableTypeB


def okay(foo: ImmutableTypeB = []):
    ...


def okay(foo: custom.ImmutableTypeA = []):
    ...


def okay(foo: custom.ImmutableTypeB = []):
    ...


def error_due_to_missing_import(foo: ImmutableTypeA = []):
    ...
