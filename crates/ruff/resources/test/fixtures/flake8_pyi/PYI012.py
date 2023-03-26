# Violations

class Foo:  # PYI012
    pass

class Foo:  # PYI012
    """buzz"""

    pass

class Bar: pass  # PYI012

class Baz:  # PYI012
    """
    My body only contains pass.
    """
    pass


class Error(Exception):  # PYI012
    pass


class BadWithInit:  # PYI012 
    value: int
    pass
    def __init__():
        prop: str = "property"

class OneAttr:  # PYI012
    value: int
    pass

class OneAttrRev:  # PYI012
    pass
    value: int




# Not violations

class Dog: 
    eyes: int = 2


class Pass: 
    ...

class Pazz: ...

class WithInit:  
    value: int = 0
    def __init__():
        prop: str = "property"
        pass

def function():
    pass

pass

