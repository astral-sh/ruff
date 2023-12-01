def test():
        # leading comment
    """ This docstring does not
        get formatted
    """ # fmt: skip
        # trailing comment

def test():
    # leading comment
    """   This docstring gets formatted
    """ # trailing comment

    and_this +  gets + formatted + too
