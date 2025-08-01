SEQ = "1,2,3"

class Foo(str):
    class_str = "1,2,3"
    
    def split(self, sep=None, maxsplit=-1) -> list[str]:
        return super().split(sep, maxsplit)

class Bar():
    split = "1,2,3"

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
"1,2,3"[::-1][::-1].split(",")[0]  # [missing-maxsplit-arg]
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

## Test class attribute named split
Bar.split.split(",")[0]  # [missing-maxsplit-arg]
Bar.split.split(",")[-1]  # [missing-maxsplit-arg]
Bar.split.rsplit(",")[0]  # [missing-maxsplit-arg]
Bar.split.rsplit(",")[-1]  # [missing-maxsplit-arg]

## Test unpacked dict literal kwargs 
"1,2,3".split(**{"sep": ","})[0]  # [missing-maxsplit-arg]


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
"1,2,3"[::-1].split(",")[1]
SEQ[:3].split(",")[1]
Foo.class_str[1:3].split(",")[-2]
"1,2,3"[::-1].rsplit(",")[1]
SEQ[:3].rsplit(",")[1]
Foo.class_str[1:3].rsplit(",")[-2]

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

## Test class attribute named split
Bar.split[0]
Bar.split[-1]
Bar.split[0]
Bar.split[-1]

## Test unpacked dict literal kwargs 
"1,2,3".split(",", **{"maxsplit": 1})[0]
"1,2,3".split(**{"sep": ",", "maxsplit": 1})[0]


# TODO

## Test variable split result index
## TODO: These require the ability to resolve a variable name to a value
# Errors
result_index = 0
"1,2,3".split(",")[result_index]  # TODO: [missing-maxsplit-arg]
result_index = -1
"1,2,3".split(",")[result_index]  # TODO: [missing-maxsplit-arg]
# OK 
result_index = 1
"1,2,3".split(",")[result_index]
result_index = -2
"1,2,3".split(",")[result_index]


## Test split result index modified in loop
## TODO: These require the ability to recognize being in a loop where:
##     - the result of split called on a string is indexed by a variable
##     - the variable index above is modified
# OK
result_index = 0
for j in range(3):
    print(SEQ.split(",")[result_index])
    result_index = result_index + 1


## Test accessor
## TODO: These require the ability to get the return type of a method
## (possibly via `typing::is_string`)
class Baz():
    def __init__(self):
        self.my_str = "1,2,3"

    def get_string(self) -> str:
        return self.my_str

# Errors
Baz().get_string().split(",")[0]  # TODO: [missing-maxsplit-arg]
Baz().get_string().split(",")[-1]  # TODO: [missing-maxsplit-arg]
# OK
Baz().get_string().split(",")[1]
Baz().get_string().split(",")[-2]


## Test unpacked dict instance kwargs
## TODO: These require the ability to resolve a dict variable name to a value
# Errors
kwargs_without_maxsplit = {"seq": ","}
"1,2,3".split(**kwargs_without_maxsplit)[0]  # TODO: [missing-maxsplit-arg]
# OK
kwargs_with_maxsplit = {"maxsplit": 1}
"1,2,3".split(",", **kwargs_with_maxsplit)[0]  # TODO: false positive
kwargs_with_maxsplit = {"sep": ",", "maxsplit": 1}
"1,2,3".split(**kwargs_with_maxsplit)[0]  # TODO: false positive


## Test unpacked list literal args (starred expressions)
# Errors
"1,2,3".split(",", *[-1])[0]

## Test unpacked list variable args
# Errors
args_list = [-1]
"1,2,3".split(",", *args_list)[0]
