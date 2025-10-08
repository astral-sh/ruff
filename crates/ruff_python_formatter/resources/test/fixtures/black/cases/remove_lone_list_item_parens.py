items = [(123)]
items = [(True)]
items = [(((((True)))))]
items = [(((((True,)))))]
items = [((((()))))]
items = [(x for x in [1])]
items = {(123)}
items = {(True)}
items = {(((((True)))))}

# Requires `hug_parens_with_braces_and_square_brackets` unstable style to remove parentheses
# around multiline values
items = [
    (
        {"key1": "val1", "key2": "val2", "key3": "val3"}
        if some_var == "long strings"
        else {"key": "val"}
    )
]
items = [
    (
        {"key1": "val1", "key2": "val2"}
        if some_var == ""
        else {"key": "val"}
    )
]

# Comments should not cause crashes
items = [
    (  # comment
        {"key1": "val1", "key2": "val2", "key3": "val3"}
        if some_var == "long strings"
        else {"key": "val"}
    )
]
items = [
    (
        {"key1": "val1", "key2": "val2", "key3": "val3"}
        if some_var == "long strings"
        else {"key": "val"}
    )  # comment
]

items = [  # comment
    (
        {"key1": "val1", "key2": "val2", "key3": "val3"}
        if some_var == "long strings"
        else {"key": "val"}
    )
]
items = [
    (
        {"key1": "val1", "key2": "val2", "key3": "val3"}
        if some_var == "long strings"
        else {"key": "val"}
    )
]  # comment

items = [
    (
        {"key1": "val1", "key2": "val2", "key3": "val3"}  # comment
        if some_var == "long strings"
        else {"key": "val"}
    )
]

items = [  # comment
    (  # comment
        {"key1": "val1", "key2": "val2", "key3": "val3"}
        if some_var == "long strings"
        else {"key": "val"}
    )
]
items = [
    (
        {"key1": "val1", "key2": "val2", "key3": "val3"}
        if some_var == "long strings"
        else {"key": "val"}
    )  # comment
]  # comment
