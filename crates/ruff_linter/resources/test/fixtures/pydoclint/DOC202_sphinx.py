# OK
def foo(num: int) -> str:
    """
    Do something

    :param num: A number
    :type num: int
    """
    print('test')


# DOC202
def foo(num: int) -> str:
    """
    Do something

    :param num: A number
    :type num: int
    :return: A string
    :rtype: str
    """
    print('test')


class Bar:

    # DOC202
    def foo(self) -> str:
        """
        Do something

        :param num: A number
        :type num: int
        :return: A string
        :rtype: str
        """
        print('test')


    # OK
    def bar(self) -> str:
        """
        Do something

        :param num: A number
        :type num: int
        """
        print('test')
