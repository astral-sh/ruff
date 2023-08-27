class TestTypeParam[ T ]: # fmt: skip
    pass

class TestTypeParam [ # trailing open paren comment
    # leading comment
    T # trailing type param comment
    # trailing type param own line comment
]: # fmt: skip
    pass

class TestTrailingComment4[
    T
]  ( # trailing arguments open parenthesis comment
    # leading argument comment
    A # trailing argument comment
    # trailing argument own line comment
): # fmt: skip
    pass

def test  [
    # comment
    A,

    # another

    B,
]  (): # fmt: skip
    ...

def test  [
    # comment
    A,

    # another

    B,
]  () -> str: # fmt: skip
    ...
