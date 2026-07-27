# `non-callable` (`PLE1102`)

```toml
[lint]
preview = true
select = ["PLE1102"]
```

## Callables

Cases of callables that shouldn't trigger a diagnostic.
The only guaranteed callable here is `lambda` in all other cases we just can't be certain and therefore allow it.

```python

# Lambda
(lambda: None)()

# Name
foo()

# Call
foo()()

# Attribute
foo.bar()

# Subscript
foo[0]()

# Await
async def f():
    (await foo())()

```

## Errors + SyntaxWarnings

All basic cases of non-callables, all emit SyntaxWarning error during Python compilation and result in `TypeError` at runtime.

```python
# String
"foo"()  # snapshot: non-callable

# F-string
f"foo"()  # snapshot: non-callable

# Bytes
b"foo"()  # snapshot: non-callable

# Number (int)
(1)()  # snapshot: non-callable

# Number (float)
(1.5)()  # snapshot: non-callable

# Number (complex)
(1j)()  # snapshot: non-callable

# Number (bool)
True()  # snapshot: non-callable

# None
None()  # snapshot: non-callable

# Ellipsis
(...)()  # snapshot: non-callable

# Dict
{"a": 1}()  # snapshot: non-callable

# Dict comprehension
{k: v for k, v in [("a", 1)]}()  # snapshot: non-callable

# List
[1, 2, 3]()  # snapshot: non-callable

# List comprehension
[i for i in range(5)]()  # snapshot: non-callable

# Set
{1, 2, 3}()  # snapshot: non-callable

# Set comprehension
{i for i in range(5)}()  # snapshot: non-callable

# Tuple
(1, 2, 3)()  # snapshot: non-callable

# Generator
(i for i in range(5))()  # snapshot: non-callable

# Template
t"hello {name}"()  # snapshot: non-callable

```

```snapshot
error[PLE1102]: `str` object is not callable.
 --> src/mdtest_snippet.py:2:1
  |
2 | "foo"()  # snapshot: non-callable
  | ^^^^^
  |


error[PLE1102]: `str` object is not callable.
 --> src/mdtest_snippet.py:5:1
  |
5 | f"foo"()  # snapshot: non-callable
  | ^^^^^^
  |


error[PLE1102]: `bytes` object is not callable.
 --> src/mdtest_snippet.py:8:1
  |
8 | b"foo"()  # snapshot: non-callable
  | ^^^^^^
  |


error[PLE1102]: `int` object is not callable.
  --> src/mdtest_snippet.py:11:2
   |
11 | (1)()  # snapshot: non-callable
   |  ^
   |


error[PLE1102]: `float` object is not callable.
  --> src/mdtest_snippet.py:14:2
   |
14 | (1.5)()  # snapshot: non-callable
   |  ^^^
   |


error[PLE1102]: `complex` object is not callable.
  --> src/mdtest_snippet.py:17:2
   |
17 | (1j)()  # snapshot: non-callable
   |  ^^
   |


error[PLE1102]: `bool` object is not callable.
  --> src/mdtest_snippet.py:20:1
   |
20 | True()  # snapshot: non-callable
   | ^^^^
   |


error[PLE1102]: `NoneType` object is not callable.
  --> src/mdtest_snippet.py:23:1
   |
23 | None()  # snapshot: non-callable
   | ^^^^
   |


error[PLE1102]: `ellipsis` object is not callable.
  --> src/mdtest_snippet.py:26:2
   |
26 | (...)()  # snapshot: non-callable
   |  ^^^
   |


error[PLE1102]: `dict` object is not callable.
  --> src/mdtest_snippet.py:29:1
   |
29 | {"a": 1}()  # snapshot: non-callable
   | ^^^^^^^^
   |


error[PLE1102]: `dict` object is not callable.
  --> src/mdtest_snippet.py:32:1
   |
32 | {k: v for k, v in [("a", 1)]}()  # snapshot: non-callable
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |


error[PLE1102]: `list` object is not callable.
  --> src/mdtest_snippet.py:35:1
   |
35 | [1, 2, 3]()  # snapshot: non-callable
   | ^^^^^^^^^
   |


error[PLE1102]: `list` object is not callable.
  --> src/mdtest_snippet.py:38:1
   |
38 | [i for i in range(5)]()  # snapshot: non-callable
   | ^^^^^^^^^^^^^^^^^^^^^
   |


error[PLE1102]: `set` object is not callable.
  --> src/mdtest_snippet.py:41:1
   |
41 | {1, 2, 3}()  # snapshot: non-callable
   | ^^^^^^^^^
   |


error[PLE1102]: `set` object is not callable.
  --> src/mdtest_snippet.py:44:1
   |
44 | {i for i in range(5)}()  # snapshot: non-callable
   | ^^^^^^^^^^^^^^^^^^^^^
   |


error[PLE1102]: `tuple` object is not callable.
  --> src/mdtest_snippet.py:47:1
   |
47 | (1, 2, 3)()  # snapshot: non-callable
   | ^^^^^^^^^
   |


error[PLE1102]: `generator` object is not callable.
  --> src/mdtest_snippet.py:50:1
   |
50 | (i for i in range(5))()  # snapshot: non-callable
   | ^^^^^^^^^^^^^^^^^^^^^
   |


error[PLE1102]: `Template` object is not callable.
  --> src/mdtest_snippet.py:53:1
   |
53 | t"hello {name}"()  # snapshot: non-callable
   | ^^^^^^^^^^^^^^^
   |
```

## Errors caught using type inference

Given that we have a small type inference engine it allows us to catch some more complex non-callables. Those would also result in `TypeError` at runtime, but they do not emit `SyntaxWarning` during compilation.

```python
# BinOp
("a" + "b")()  # snapshot: non-callable

# If (Union)
(1 if True else "a")()  # snapshot: non-callable

# UnaryOp
(not 1)()  # snapshot: non-callable

# BoolOp
(True or False)()  # snapshot: non-callable
```

```snapshot
error[PLE1102]: `str` object is not callable.
 --> src/mdtest_snippet.py:2:2
  |
2 | ("a" + "b")()  # snapshot: non-callable
  |  ^^^^^^^^^
  |


error[PLE1102]: `str | int` object is not callable.
 --> src/mdtest_snippet.py:5:2
  |
5 | (1 if True else "a")()  # snapshot: non-callable
  |  ^^^^^^^^^^^^^^^^^^
  |


error[PLE1102]: `bool` object is not callable.
 --> src/mdtest_snippet.py:8:2
  |
8 | (not 1)()  # snapshot: non-callable
  |  ^^^^^
  |


error[PLE1102]: `bool` object is not callable.
  --> src/mdtest_snippet.py:11:2
   |
11 | (True or False)()  # snapshot: non-callable
   |  ^^^^^^^^^^^^^
   |
```
