# DOC104
def add_with_violation(a, b) -> int:
    """
    Adds two numbers and returns the result.

    Parameters
    ----------
    b : int
        The second number to add.
    a :int
        The first number to add.

    Returns
    -------
    int
        The sum of the two numbers.
    """
    return a + b


# OK
def add_numbers(a, b) -> int:
    """
    Adds two numbers and returns the result.

    Parameters
    ----------
    a : int
        The first number to add.
    b : int
        The second number to add.

    Returns
    -------
    int
        The sum of the two numbers.
    """
    return a + b


class Calculator:

    # DOC104
    def add_numbers_incorrect(self, a, b) -> int:
        """
        Adds two numbers and returns the result.

        Parameters
        ----------
        b : int
            The second number to add.
        a : int
            The first number to add.

        Returns
        -------
        int
            The sum of the two numbers.
        """
        return a + b

    # OK
    def add_numbers(self, a, b) -> int:
        """
        Adds two numbers and returns the result.

        Parameters
        ----------
        a : int
            The first number to add.
        b : int
            The second number to add.

        Returns
        -------
        int
            The sum of the two numbers.
        """
        return a + b


# OK
def test(a, b):
    """
    Do something.

    Parameters
    ----------
    a : int
        Some number.
    b : int
        Another number.
    """
    # DOC104
    def nested(a, b):
        """
        Do something nested.

        Parameters
        ----------
        b : int
            Another number.
        a : int
            Some number.
        """
        print('Do something')

    print('Outer print')
