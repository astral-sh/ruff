def test():
    # fmt: off
    """ This docstring does not
        get formatted
    """

    # fmt: on

    but + this  + does

def test():
    # fmt: off
        # just for fun
    # fmt: on
        # leading comment
    """   This docstring gets formatted
    """ # trailing comment

    and_this +  gets + formatted + too
