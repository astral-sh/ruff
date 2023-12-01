class BadAllowed(Exception):
    pass

class StillBad(Exception):
    pass

class BadAllowed(AnotherError):
    pass

class StillBad(AnotherError):
    pass
