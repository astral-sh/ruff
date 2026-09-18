# `redefined-loop-name` (`PLW2901`)

```toml
[lint]
select = ["PLW2901"]
```

## Augmented assignment

Ignore in-place update of a mutable type.

```py
for i in []:
    i += [1]

for i in []:
    i = [1]  # snapshot: redefined-loop-name

for i in []:
    i |= {"a": 1}

for i in []:
    i = {"b": 2}  # snapshot: redefined-loop-name

for i in []:
    i |= {1}

for i in []:
    i &= {1}

for i in []:
    i ^= {1}

for i in []:
    i -= {1}

for i in []:
    i = {1} # snapshot: redefined-loop-name

for i in []:
    i += (1,)  # snapshot: redefined-loop-name

for i in []:
    i += "a"  # snapshot: redefined-loop-name
```

```snapshot
error[PLW2901]: `for` loop variable `i` overwritten by assignment target
 --> src/mdtest_snippet.py:5:5
  |
5 |     i = [1]  # snapshot: redefined-loop-name
  |     ^


error[PLW2901]: `for` loop variable `i` overwritten by assignment target
  --> src/mdtest_snippet.py:11:5
   |
11 |     i = {"b": 2}  # snapshot: redefined-loop-name
   |     ^


error[PLW2901]: `for` loop variable `i` overwritten by assignment target
  --> src/mdtest_snippet.py:26:5
   |
26 |     i = {1} # snapshot: redefined-loop-name
   |     ^


error[PLW2901]: `for` loop variable `i` overwritten by assignment target
  --> src/mdtest_snippet.py:29:5
   |
29 |     i += (1,)  # snapshot: redefined-loop-name
   |     ^


error[PLW2901]: `for` loop variable `i` overwritten by assignment target
  --> src/mdtest_snippet.py:32:5
   |
32 |     i += "a"  # snapshot: redefined-loop-name
   |     ^
```
