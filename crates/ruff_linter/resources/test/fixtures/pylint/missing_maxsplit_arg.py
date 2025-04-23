SEQ = "1,2,3"

class Foo(str):
    class_str = "1,2,3"
    
    def split(self, sep=None, maxsplit=-1) -> list[str]:
        return super().split(sep, maxsplit)

# Errors
## Test split called directly on string literal
get_first = "1,2,3".split(",")[0]  # [missing-maxsplit-arg]
get_last = "1,2,3".split(",")[-1]  # [missing-maxsplit-arg]
get_first = "1,2,3".rsplit(",")[0]  # [missing-maxsplit-arg]
get_last = "1,2,3".rsplit(",")[-1]  # [missing-maxsplit-arg]

## Test split called on string variable
get_first = SEQ.split(",")[0]  # [missing-maxsplit-arg]
get_last = SEQ.split(",")[-1]  # [missing-maxsplit-arg]
get_first = SEQ.rsplit(",")[0]  # [missing-maxsplit-arg]
get_last = SEQ.rsplit(",")[-1]  # [missing-maxsplit-arg]

## Test split called on class attribute
get_first = Foo.class_str.split(",")[0]  # [missing-maxsplit-arg]
get_last = Foo.class_str.split(",")[-1]  # [missing-maxsplit-arg]
get_first = Foo.class_str.rsplit(",")[0]  # [missing-maxsplit-arg]
get_last = Foo.class_str.rsplit(",")[-1]  # [missing-maxsplit-arg]

## Test sep given as named argument
get_first = "1,2,3".split(sep=",")[0]  # [missing-maxsplit-arg]
get_last = "1,2,3".split(sep=",")[-1]  # [missing-maxsplit-arg]
get_first = "1,2,3".rsplit(sep=",")[0]  # [missing-maxsplit-arg]
get_last = "1,2,3".rsplit(sep=",")[-1]  # [missing-maxsplit-arg]

## Special cases
a = "1,2,3".split("\n")[0]  # [missing-maxsplit-arg]
a = "1,2,3".split("split")[-1]  # [missing-maxsplit-arg]
a = "1,2,3".rsplit("rsplit")[0]  # [missing-maxsplit-arg]

# OK
## Don't suggest maxsplit=1 if not accessing the first or last element
get_mid = "1,2,3".split(",")[1]
get_mid = "1,2,3".split(",")[-2]

## Test varying maxsplit argument -- all these will be okay
### str.split() tests
good_split = "1,2,3".split(sep=",", maxsplit=1)[-1]
good_split = "1,2,3".split(sep=",", maxsplit=1)[0]
good_split = "1,2,3".split(sep=",", maxsplit=2)[-1]
good_split = "1,2,3".split(sep=",", maxsplit=2)[0]
good_split = "1,2,3".split(sep=",", maxsplit=2)[1]

### str.rsplit() tests
good_split = "1,2,3".rsplit(sep=",", maxsplit=1)[-1]
good_split = "1,2,3".rsplit(sep=",", maxsplit=1)[0]
good_split = "1,2,3".rsplit(sep=",", maxsplit=2)[-1]
good_split = "1,2,3".rsplit(sep=",", maxsplit=2)[0]
good_split = "1,2,3".rsplit(sep=",", maxsplit=2)[1]

## Test class attributes
get_mid = Foo.class_str.split(",")[1]
get_mid = Foo.class_str.split(",")[-2]

## Test user-defined split
get_first = Foo("1,2,3").split(",")[0]
get_last = Foo("1,2,3").split(",")[-1]


# TODO

# class Bar():
#     def __init__(self):
#         self.my_str = "1,2,3"

#     def get_string(self) -> str:
#         return self.my_str


# class Baz():
#     split = "1,2,3"

# Errors
## Test split called on reverse sliced string
# get_last = "1,2,3"[::-1].split(",")[0]  # [missing-maxsplit-arg]
## Test sep arg from unpacked dict (without maxsplit arg)
# bad_split = "1,2,3".split(**{"sep": ","})
## Test accessor
# get_first = Bar().get_string().split(",")[0]  # [use-maxsplit-arg]
# get_last = Bar().get_string().split(",")[-1]  # [use-maxsplit-arg]
## Error message should show Bar.split.split(",", maxsplit=1) or Bar.split.rsplit(",", maxsplit=1)
# print(Baz.split.split(",")[0])  # [missing-maxsplit-arg]
# print(Baz.split.split(",")[-1])  # [missing-maxsplit-arg]
# print(Baz.split.rsplit(",")[0])  # [missing-maxsplit-arg]
# print(Baz.split.rsplit(",")[-1])  # [missing-maxsplit-arg]

# OK
## Test maxsplit arg modified in loop
# i = 0
# for j in range(3):
#     print(SEQ.split(",")[i])
#     i = i + 1
## Test maxsplit arg from unpacked dict
# good_split = "1,2,3".split(",", **{"maxsplit": 2})[0]
## Test split args from unpacked dict
# good_split = "1,2,3".split(**{"sep": ",", "maxsplit": 2})[0]
## Test accessor
# get_mid = Bar().get_string().split(",")[1]
# get_mid = Bar().get_string().split(",")[-2]