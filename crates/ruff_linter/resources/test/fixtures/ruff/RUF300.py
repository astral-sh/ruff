from typing import NoReturn


def valid1():  # OK
    return 123


def valid2() -> int:  # OK
    return 123


def valid3() -> None:  # OK
    return


def valid4():  # OK
    yield 123
    return


def valid5():  # OK
    raise NotImplementedError()


def valid5() -> NoReturn:  # OK
    raise ValueError()


def invalid1():  # RUF300
    return None


def invalid2():  # RUF300
    print()


def wrong_annotation() -> int:  # RUF300
    print()


def invalid3():  # RUF300
    return (None)


async def invalid4():  # RUF300
    return  # TODO is this correct?
