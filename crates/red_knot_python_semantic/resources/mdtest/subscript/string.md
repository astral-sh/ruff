# String subscripts

## Indexing

```py
s = "abcde"

reveal_type(s[0])  # revealed: Literal["a"]
reveal_type(s[1])  # revealed: Literal["b"]
reveal_type(s[-1])  # revealed: Literal["e"]
reveal_type(s[-2])  # revealed: Literal["d"]

reveal_type(s[False])  # revealed: Literal["a"]
reveal_type(s[True])  # revealed: Literal["b"]

a = s[8]  # error: [index-out-of-bounds] "Index 8 is out of bounds for string `Literal["abcde"]` with length 5"
reveal_type(a)  # revealed: Unknown

b = s[-8]  # error: [index-out-of-bounds] "Index -8 is out of bounds for string `Literal["abcde"]` with length 5"
reveal_type(b)  # revealed: Unknown

def _(n: int):
    a = "abcde"[n]
    # TODO: Support overloads... Should be `str`
    reveal_type(a)  # revealed: @Todo(return type of overloaded function)
```

## Slices

```py
def _(m: int, n: int, s2: str):
    s = "abcde"

    reveal_type(s[0:0])  # revealed: Literal[""]
    reveal_type(s[0:1])  # revealed: Literal["a"]
    reveal_type(s[0:2])  # revealed: Literal["ab"]
    reveal_type(s[0:5])  # revealed: Literal["abcde"]
    reveal_type(s[0:6])  # revealed: Literal["abcde"]
    reveal_type(s[1:3])  # revealed: Literal["bc"]

    reveal_type(s[-3:5])  # revealed: Literal["cde"]
    reveal_type(s[-4:-2])  # revealed: Literal["bc"]
    reveal_type(s[-10:10])  # revealed: Literal["abcde"]

    reveal_type(s[0:])  # revealed: Literal["abcde"]
    reveal_type(s[2:])  # revealed: Literal["cde"]
    reveal_type(s[5:])  # revealed: Literal[""]
    reveal_type(s[:2])  # revealed: Literal["ab"]
    reveal_type(s[:0])  # revealed: Literal[""]
    reveal_type(s[:2])  # revealed: Literal["ab"]
    reveal_type(s[:10])  # revealed: Literal["abcde"]
    reveal_type(s[:])  # revealed: Literal["abcde"]

    reveal_type(s[::-1])  # revealed: Literal["edcba"]
    reveal_type(s[::2])  # revealed: Literal["ace"]
    reveal_type(s[-2:-5:-1])  # revealed: Literal["dcb"]
    reveal_type(s[::-2])  # revealed: Literal["eca"]
    reveal_type(s[-1::-3])  # revealed: Literal["eb"]

    reveal_type(s[None:2:None])  # revealed: Literal["ab"]
    reveal_type(s[1:None:1])  # revealed: Literal["bcde"]
    reveal_type(s[None:None:None])  # revealed: Literal["abcde"]

    start = 1
    stop = None
    step = 2
    reveal_type(s[start:stop:step])  # revealed: Literal["bd"]

    reveal_type(s[False:True])  # revealed: Literal["a"]
    reveal_type(s[True:3])  # revealed: Literal["bc"]

    s[0:4:0]  # error: [zero-stepsize-in-slice]
    s[:4:0]  # error: [zero-stepsize-in-slice]
    s[0::0]  # error: [zero-stepsize-in-slice]
    s[::0]  # error: [zero-stepsize-in-slice]

    substring1 = s[m:n]
    # TODO: Support overloads... Should be `LiteralString`
    reveal_type(substring1)  # revealed: @Todo(return type of overloaded function)

    substring2 = s2[0:5]
    # TODO: Support overloads... Should be `str`
    reveal_type(substring2)  # revealed: @Todo(return type of overloaded function)
```

## Unsupported slice types

```py
# TODO: It would be great if we raised an error here. This can be done once
# we have support for overloads and generics, and once typeshed has a more
# precise annotation for `str.__getitem__`, that makes use of the generic
# `slice[..]` type. We could then infer `slice[str, str]` here and see that
# it doesn't match the signature of `str.__getitem__`.
"foo"["bar":"baz"]
```
