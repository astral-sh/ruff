# DOC201
def foo(num: int) -> str:
    """
    Do something

    :param num: A number
    :type num: int
    """
    return 'test'


# OK
def foo(num: int) -> str:
    """
    Do something

    :param num: A number
    :type num: int
    :return: A string
    :rtype: str
    """
    return 'test'


class Bar:

    # OK
    def foo(self) -> str:
        """
        Do something

        :param num: A number
        :type num: int
        :return: A string
        :rtype: str
        """
        return 'test'


    # DOC201
    def bar(self) -> str:
        """
        Do something

        :param num: A number
        :type num: int
        """
        return 'test'


    # OK
    @property
    def baz(self) -> str:
        """
        Do something

        :param num: A number
        :type num: int
        """
        return 'test'
