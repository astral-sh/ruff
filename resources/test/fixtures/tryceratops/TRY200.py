class MyException(Exception):
    pass


class MainFunctionFailed(Exception):
    pass


def process():
    raise MyException


def bad():
    try:
        process()
    except MyException:
        raise MainFunctionFailed()

        if True:
            raise MainFunctionFailed()


def good():
    try:
        process()
    except MyException as ex:
        raise MainFunctionFailed() from ex
