pierogi_fillings = [
    "cabbage",
    "strawberry",
    "cheese",
    "blueberry",
]

# Errors.
dict.fromkeys(pierogi_fillings, [])
dict.fromkeys(pierogi_fillings, list())
dict.fromkeys(pierogi_fillings, {})
dict.fromkeys(pierogi_fillings, set())
dict.fromkeys(pierogi_fillings, {"pre": "populated!"})
dict.fromkeys(pierogi_fillings, dict())
import builtins
builtins.dict.fromkeys(pierogi_fillings, dict())

# Okay.
dict.fromkeys(pierogi_fillings)
dict.fromkeys(pierogi_fillings, None)
dict.fromkeys(pierogi_fillings, 1)
dict.fromkeys(pierogi_fillings)
dict.fromkeys(pierogi_fillings, ("blessed", "tuples", "don't", "mutate"))
dict.fromkeys(pierogi_fillings, "neither do strings")

dict.fromkeys(pierogi_fillings, (x for x in "cv"))
dict.fromkeys(pierogi_fillings, (x := []))
dict.fromkeys(pierogi_fillings, ([],))

class MysteryBox: ...

dict.fromkeys(pierogi_fillings, MysteryBox)
bar.fromkeys(pierogi_fillings, [])


def bad_dict() -> None:
    dict = MysteryBox()
    dict.fromkeys(pierogi_fillings, [])
