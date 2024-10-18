# String literals

## Simple

```py
reveal_type("Hello")  # revealed: Literal["Hello"]
reveal_type("world")  # revealed: Literal["world"]
reveal_type("Guten " + "Tag")  # revealed: Literal["Guten Tag"]
reveal_type("bon " + "jour")  # revealed: Literal["bon jour"]
```

## Nested Quotes

```py
reveal_type('I say "hello" to you')  # revealed: Literal["I say \"hello\" to you"]
reveal_type('You say "hey" back')  # revealed: Literal["You say \"hey\" back"]
reveal_type('No "closure here')  # revealed: Literal["No \"closure here"]
```
