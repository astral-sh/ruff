class C:
    badAllowed = 0
    stillBad = 0

    _badAllowed = 0
    _stillBad = 0

    bad_Allowed = 0
    still_Bad = 0

class D(TypedDict):
    badAllowed: bool
    stillBad: bool

    _badAllowed: list
    _stillBad: list

    bad_Allowed: set
    still_Bad: set
