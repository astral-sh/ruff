def __bad__():
    pass


def __good():
    pass


def good__():
    pass


def nested():
    def __bad__():
        pass

    def __good():
        pass

    def good__():
        pass


class Class:
    def __good__(self):
        pass


# https://peps.python.org/pep-0562/
def __getattr__(name):
    pass


# https://peps.python.org/pep-0562/
def __dir__():
    pass
