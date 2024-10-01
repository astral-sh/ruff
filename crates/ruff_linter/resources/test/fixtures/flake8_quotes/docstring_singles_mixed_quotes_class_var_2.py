class SingleLineDocstrings():
    'Do not'" start with empty string" ' and lint docstring safely'
    ''' Not a docstring '''

    def foo(self, bar='''not a docstring'''):
        'Do not'" start with empty string" ' and lint docstring safely'
        pass

    class Nested(foo()[:]): 'Do not'" start with empty string" ' and lint docstring safely'; pass
