class SingleLineDocstrings():
    """ Double quotes single line class docstring """
    """ Not a docstring """

    def foo(self, bar="""not a docstring"""):
        """ Double quotes single line method docstring"""
        pass

    class Nested(foo()[:]): """ inline docstring """; pass
