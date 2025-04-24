SEQ = "1,2,3"

class Foo(str):
    class_str = "1,2,3"
    
    def split(self, sep=None, maxsplit=-1) -> list[str]:
        return super().split(sep, maxsplit)

# Errors
## Test split called directly on string literal
"1,2,3".split(",")[0]  # [missing-maxsplit-arg]
"1,2,3".split(",")[-1]  # [missing-maxsplit-arg]
"1,2,3".rsplit(",")[0]  # [missing-maxsplit-arg]
"1,2,3".rsplit(",")[-1]  # [missing-maxsplit-arg]

## Test split called on string variable
SEQ.split(",")[0]  # [missing-maxsplit-arg]
SEQ.split(",")[-1]  # [missing-maxsplit-arg]
SEQ.rsplit(",")[0]  # [missing-maxsplit-arg]
SEQ.rsplit(",")[-1]  # [missing-maxsplit-arg]

## Test split called on class attribute
Foo.class_str.split(",")[0]  # [missing-maxsplit-arg]
Foo.class_str.split(",")[-1]  # [missing-maxsplit-arg]
Foo.class_str.rsplit(",")[0]  # [missing-maxsplit-arg]
Foo.class_str.rsplit(",")[-1]  # [missing-maxsplit-arg]

## Test split called on sliced string
"1,2,3"[::-1].split(",")[0]  # [missing-maxsplit-arg]
SEQ[:3].split(",")[0]  # [missing-maxsplit-arg]
Foo.class_str[1:3].split(",")[-1]  # [missing-maxsplit-arg]
"1,2,3"[::-1].rsplit(",")[0]  # [missing-maxsplit-arg]
SEQ[:3].rsplit(",")[0]  # [missing-maxsplit-arg]
Foo.class_str[1:3].rsplit(",")[-1]  # [missing-maxsplit-arg]

## Test sep given as named argument
"1,2,3".split(sep=",")[0]  # [missing-maxsplit-arg]
"1,2,3".split(sep=",")[-1]  # [missing-maxsplit-arg]
"1,2,3".rsplit(sep=",")[0]  # [missing-maxsplit-arg]
"1,2,3".rsplit(sep=",")[-1]  # [missing-maxsplit-arg]

## Special cases
"1,2,3".split("\n")[0]  # [missing-maxsplit-arg]
"1,2,3".split("split")[-1]  # [missing-maxsplit-arg]
"1,2,3".rsplit("rsplit")[0]  # [missing-maxsplit-arg]


# OK
## Test not accessing the first or last element
### Test split called directly on string literal
"1,2,3".split(",")[1]
"1,2,3".split(",")[-2]
"1,2,3".rsplit(",")[1]
"1,2,3".rsplit(",")[-2]

### Test split called on string variable
SEQ.split(",")[1]
SEQ.split(",")[-2]
SEQ.rsplit(",")[1]
SEQ.rsplit(",")[-2]

### Test split called on class attribute
Foo.class_str.split(",")[1]
Foo.class_str.split(",")[-2]
Foo.class_str.rsplit(",")[1]
Foo.class_str.rsplit(",")[-2]

### Test split called on sliced string
"1,2,3"[::-1].split(",")[1]  # [missing-maxsplit-arg]
SEQ[:3].split(",")[1]  # [missing-maxsplit-arg]
Foo.class_str[1:3].split(",")[-2]  # [missing-maxsplit-arg]
"1,2,3"[::-1].rsplit(",")[1]  # [missing-maxsplit-arg]
SEQ[:3].rsplit(",")[1]  # [missing-maxsplit-arg]
Foo.class_str[1:3].rsplit(",")[-2]  # [missing-maxsplit-arg]

### Test sep given as named argument
"1,2,3".split(sep=",")[1]
"1,2,3".split(sep=",")[-2]
"1,2,3".rsplit(sep=",")[1]
"1,2,3".rsplit(sep=",")[-2]

## Test varying maxsplit argument
### str.split() tests
"1,2,3".split(sep=",", maxsplit=1)[-1]
"1,2,3".split(sep=",", maxsplit=1)[0]
"1,2,3".split(sep=",", maxsplit=2)[-1]
"1,2,3".split(sep=",", maxsplit=2)[0]
"1,2,3".split(sep=",", maxsplit=2)[1]

### str.rsplit() tests
"1,2,3".rsplit(sep=",", maxsplit=1)[-1]
"1,2,3".rsplit(sep=",", maxsplit=1)[0]
"1,2,3".rsplit(sep=",", maxsplit=2)[-1]
"1,2,3".rsplit(sep=",", maxsplit=2)[0]
"1,2,3".rsplit(sep=",", maxsplit=2)[1]

## Test user-defined split
Foo("1,2,3").split(",")[0]
Foo("1,2,3").split(",")[-1]
Foo("1,2,3").rsplit(",")[0]
Foo("1,2,3").rsplit(",")[-1]

## Test split called on sliced list
["1", "2", "3"][::-1].split(",")[0]


# TODO

# class Bar():
#     def __init__(self):
#         self.my_str = "1,2,3"

#     def get_string(self) -> str:
#         return self.my_str


# class Baz():
#     split = "1,2,3"

# Errors
## Test sep arg from unpacked dict (without maxsplit arg)
# "1,2,3".split(**{"sep": ","})
## Test accessor
# Bar().get_string().split(",")[0]  # [use-maxsplit-arg]
# Bar().get_string().split(",")[-1]  # [use-maxsplit-arg]
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
# "1,2,3".split(",", **{"maxsplit": 2})[0]
## Test split args from unpacked dict
# "1,2,3".split(**{"sep": ",", "maxsplit": 2})[0]
## Test accessor
# Bar().get_string().split(",")[1]
# Bar().get_string().split(",")[-2]