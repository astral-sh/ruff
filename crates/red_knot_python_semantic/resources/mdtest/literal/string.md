# String literals

## Simple

```py
w = "Hello"
x = 'world'
y = "Guten " + 'tag'
z = 'bon ' + "jour"

reveal_type(w)  # revealed: Literal["Hello"]
reveal_type(x)  # revealed: Literal["world"]
reveal_type(y)  # revealed: Literal["Guten tag"]
reveal_type(z)  # revealed: Literal["bon jour"]
```

## Nested Quotes

```py
x = 'I say "hello" to you'
y = "You say \"hey\" back"
z = 'No "closure here'
reveal_type(x)  # revealed: Literal["I say \"hello\" to you"]
reveal_type(y)  # revealed: Literal["You say \"hey\" back"]
reveal_type(z)  # revealed: Literal["No \"closure here"]
```
