# `block-variable-shadows-local`

```toml
lint.preview = true
lint.select = ["block-variable-shadows-local"]
```

## Basic errors

A `for` loop that reuses the name of an earlier local variable overwrites it, so code after the loop
sees the loop's last value instead.

```py
defects = get_defects()

for path, defects in snippets.items():  # snapshot: block-variable-shadows-local
    report(path, defects)

save(defects)
```

```snapshot
error[block-variable-shadows-local]: Loop variable `defects` shadows a local variable
 --> src/mdtest_snippet.py:3:11
  |
1 | defects = get_defects()
  | ------- `defects` previously assigned here
2 |
3 | for path, defects in snippets.items():  # snapshot: block-variable-shadows-local
  |           ^^^^^^^ `defects` overwritten here
help: Rename the loop variable or the local variable
```

## Loop variables

We flag shadowing anywhere in a loop target, including nested and starred targets. When a target
binds several names, only the names that shadow a local variable are flagged.

```py
def loop_variable():
    total = 0
    for total in values:  # error: [block-variable-shadows-local]
        pass
    return total


def default_for_empty_loop(start):
    index = start
    for index in range(start, stop):  # error: [block-variable-shadows-local]
        if done(index):
            break
    return index


def nested_target():
    item = None
    for idx, (key, item) in enumerate(pairs):  # error: [block-variable-shadows-local]
        pass
    return item


def starred_target(rows):
    rest = None
    for first, *rest in rows:  # error: [block-variable-shadows-local]
        pass
    return rest


def target_with_several_names(pairs):
    left = get_left()
    right = get_right()
    for left, right in pairs:  # error: [block-variable-shadows-local]
        pass
    return left


async def async_loop():
    result = []
    async for result in stream():  # error: [block-variable-shadows-local]
        pass
    return result


def annotated_assignment():
    count: int = 0
    for count in counts:  # error: [block-variable-shadows-local]
        pass
    return count


def shadowed_inside_loop_body():
    for batch in batches:
        defects = []
        for defects in batch:  # error: [block-variable-shadows-local]
            pass
        report(defects)


class Config:
    option = "default"
    for option in options:  # error: [block-variable-shadows-local]
        pass
    default = option
```

A later assignment shadows the loop variable rather than the other way around, so the loop variable
is only reported once:

```py
def reported_once():
    defects = []
    for defects in batches:  # error: [block-variable-shadows-local]
        pass
    report(defects)
    defects = []
```

## `with` targets

```py
def with_target():
    handle = open_default()
    with open("file.txt") as handle:  # error: [block-variable-shadows-local]
        pass
    return handle


async def async_with_target():
    connection = get_default_connection()
    async with connect() as connection:  # error: [block-variable-shadows-local]
        pass
    return connection


def with_tuple_target():
    reader = None
    with pair() as (reader, writer):  # error: [block-variable-shadows-local]
        pass


def with_target_reuses_context_manager():
    container = make_container()
    with container as container:  # error: [block-variable-shadows-local]
        pass
```

We also flag a `with` target that fills in a `None` placeholder, even when a nested function reads
the name and the overwrite is intentional:

```py
def placeholder_read_by_nested_function():
    progress = None

    def advance():
        if progress is not None:
            progress.update()

    with make_progress() as progress:  # error: [block-variable-shadows-local]
        run(advance)
```

## Exception names

Python deletes the exception name when the handler exits, so the earlier value is lost entirely.

```py
def exception_name():
    error = None
    try:
        pass
    except ValueError as error:  # error: [block-variable-shadows-local]
        pass
    return error
```

## Walrus assignments

A name bound by an assignment expression counts as a local variable.

```py
def walrus():
    if (match := find()) is None:
        return
    for match in matches:  # error: [block-variable-shadows-local]
        pass
    return match


def walrus_in_comprehension_read_after():
    found = [match for key in keys if (match := lookup(key)) is not None]
    for match in found:  # error: [block-variable-shadows-local]
        report(match)
    return match
```

## Loop variables that are not read after the loop

When nothing reads a loop variable after the loop, overwriting the earlier value has no observable
effect, so we don't flag it. This commonly happens when a later loop reuses the name of a temporary
from an earlier loop:

```py
def temporary_from_earlier_loop():
    threads = []
    for _ in range(5):
        thread = Thread(target=work)
        threads.append(thread)
    for thread in threads:  # no diagnostic
        thread.join()


def temporary_from_earlier_loop_unrelated():
    for event in events:
        key = event.key
        record(key)
    for key, value in mapping.items():  # no diagnostic
        report(key, value)


def temporary_from_earlier_loop_nested_block():
    for event in events:
        if event.enabled:
            key = event.key
            record(key)
    for key in keys:  # no diagnostic
        report(key)


def temporary_from_earlier_while_loop():
    while pending:
        item = pending.pop()
        process(item)
    for item in items:  # no diagnostic
        report(item)


def used_before_loop():
    fig, ax = subplots()
    ax.plot(xs, ys)
    for ax in axes:  # no diagnostic
        ax.grid()


def never_read_before_loop():
    result = []
    for result in results:  # no diagnostic
        report(result)


def walrus_never_read():
    if (match := find()) is None:
        return
    for match in matches:  # no diagnostic
        pass


def walrus_used_before_loop():
    if (match := find()) is not None:
        report(match)
    for match in matches:  # no diagnostic
        pass


def walrus_used_in_comprehension():
    found = [match for key in keys if (match := lookup(key)) is not None]
    for match in found:  # no diagnostic
        report(match)


def used_only_in_nested_function():
    metrics = ["coverage", "lines"]

    def params():
        return ",".join(metrics)

    fetch(params())
    for metrics in results:  # no diagnostic
        report(metrics)
```

A read after the loop that sees a later reassignment instead of the loop variable doesn't count:

```py
def used_before_loop_then_reassigned_after():
    response = complete(prompt)
    log(response)
    for response in responses:  # no diagnostic
        log(response)
    response = None
    return response
```

We still flag the loop variable when code after the loop reads it, even if the earlier value was
also read before the loop:

```py
def temporary_from_earlier_loop_read_after():
    for batch in batches:
        defects = load(batch)
        report(defects)
    for defects in snippets:  # error: [block-variable-shadows-local]
        pass
    save(defects)


def used_before_loop_and_read_after():
    defects = get_defects()
    report(defects)
    for defects in snippets:  # error: [block-variable-shadows-local]
        pass
    save(defects)
```

The loop's `else` clause runs after the last iteration, so a read there counts as a read after the
loop:

```py
def read_in_loop_else_clause():
    defects = get_defects()
    report(defects)
    for defects in snippets:  # error: [block-variable-shadows-local]
        pass
    else:
        save(defects)


def loop_else_clause_without_read():
    defects = get_defects()
    report(defects)
    for defects in snippets:  # no diagnostic
        if defects.critical:
            break
    else:
        report_clean()
```

A loop whose header reads the earlier value while overwriting it is also flagged:

```py
def loop_header_reads_shadowed_value():
    xs = get_xs()
    report(xs)
    for name, xs in zip(names, xs):  # error: [block-variable-shadows-local]
        pass


def loop_header_reads_augmented_value():
    xs = get_xs()
    xs += [extra]
    for name, xs in zip(names, xs):  # error: [block-variable-shadows-local]
        pass
```

## Branches

We only flag a block variable when every path to it passes through the earlier assignment. A block
nested inside a later branch still overwrites the earlier value:

```py
def loop_in_later_if_branch(condition):
    defects = get_defects()
    if condition:
        for defects in snippets:  # error: [block-variable-shadows-local]
            pass
    save(defects)


def loop_in_later_with_block():
    defects = get_defects()
    with lock:
        for defects in snippets:  # error: [block-variable-shadows-local]
            pass
    save(defects)


def loop_in_later_try_block():
    defects = get_defects()
    try:
        for defects in snippets:  # error: [block-variable-shadows-local]
            pass
    except ValueError:
        pass
    save(defects)


def loop_in_later_branch_not_read_after(condition):
    defects = get_defects()
    report(defects)
    if condition:
        for defects in snippets:  # no diagnostic
            pass
```

Bindings in different branches never hold a value at the same time, and an assignment in an earlier
branch may never have run:

```py
def different_branches(condition):
    if condition:
        defects = []
    else:
        for defects in batches:  # no diagnostic
            pass


def different_try_branches():
    try:
        result = compute()
    except ValueError:
        for result in fallbacks:  # no diagnostic
            pass


def different_match_cases(value):
    match value:
        case 1:
            result = compute()
        case _:
            for result in fallbacks:  # no diagnostic
                pass
    return result


def assignment_in_earlier_branch(condition):
    if condition:
        defects = get_defects()
    for defects in snippets:  # no diagnostic
        pass
    save(defects)
```

## Other kinds of bindings

We only flag block variables that shadow a value assigned by an ordinary assignment. Parameters are
covered by `redefined-argument-from-local` (`PLR1704`), and imports by `import-shadowed-by-loop-var`
(`F402`).

```py
import os

for os in systems:  # no diagnostic
    pass


def parameter(defects):
    for defects in batches:  # no diagnostic
        pass


def function_definition():
    def key():
        pass

    for key in keys:  # no diagnostic
        pass
    return key
```

A bare annotation declares a type without assigning a value:

```py
def bare_annotation():
    name: str
    for name in names:  # no diagnostic
        pass
```

Reusing a name for several block variables, such as the same loop variable in consecutive loops, is
allowed:

```py
def consecutive_loops():
    for i in range(10):
        pass
    for i in range(20):  # no diagnostic
        pass


def nested_with_and_loop():
    with open("file.txt") as handle:
        pass
    for handle in handles:  # no diagnostic
        pass
```

Plain reassignments and deleted variables aren't flagged either:

```py
def plain_reassignment():
    value = 1
    value = value + 1  # no diagnostic


def assignment_after_loop():
    for value in values:
        pass
    value = 0  # no diagnostic


def reassigned_in_loop_body():
    for line in lines:
        line = line.strip()  # no diagnostic


def deleted_before_loop():
    defects = []
    del defects
    for defects in batches:  # no diagnostic
        pass
```

## Dummy variables

Names matching `lint.dummy-variable-rgx` are ignored.

```py
def dummy_variable():
    _ = compute()
    for _ in range(10):  # no diagnostic
        pass
```

## Different scopes

A variable in a nested scope doesn't overwrite one in an enclosing scope.

```py
def comprehension():
    item = 1
    values = [item for item in items]  # no diagnostic


def different_scopes():
    defects = []

    def inner():
        for defects in batches:  # no diagnostic
            pass
```
