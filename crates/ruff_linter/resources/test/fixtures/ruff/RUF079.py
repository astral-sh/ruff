### Errors

defects = get_defects()

for path, defects in snippets.items():  # RUF079
    report(path, defects)

save(defects)


def loop_variable():
    total = 0
    for total in values:  # RUF079
        pass
    return total


def default_for_empty_loop(start):
    index = start
    for index in range(start, stop):  # RUF079
        if done(index):
            break
    return index


def nested_target():
    item = None
    for idx, (key, item) in enumerate(pairs):  # RUF079
        pass
    return item


def starred_target(rows):
    rest = None
    for first, *rest in rows:  # RUF079
        pass
    return rest


def target_with_several_names(pairs):
    left = get_left()
    right = get_right()
    for left, right in pairs:  # RUF079 (`left` only)
        pass
    return left


async def async_loop():
    result = []
    async for result in stream():  # RUF079
        pass
    return result


def with_target():
    handle = open_default()
    with open("file.txt") as handle:  # RUF079
        pass
    return handle


async def async_with_target():
    connection = get_default_connection()
    async with connect() as connection:  # RUF079
        pass
    return connection


def with_tuple_target():
    reader = None
    with pair() as (reader, writer):  # RUF079
        pass


def with_target_reuses_context_manager():
    container = make_container()
    with container as container:  # RUF079
        pass


def exception_name():
    error = None
    try:
        pass
    except ValueError as error:  # RUF079
        pass
    return error  # NameError: `error` is deleted when the handler exits


def walrus():
    if (match := find()) is None:
        return
    for match in matches:  # RUF079
        pass
    return match


def walrus_in_comprehension_read_after():
    found = [match for key in keys if (match := lookup(key)) is not None]
    for match in found:  # RUF079
        report(match)
    return match


def annotated_assignment():
    count: int = 0
    for count in counts:  # RUF079
        pass
    return count


def shadowed_inside_loop_body():
    for batch in batches:
        defects = []
        for defects in batch:  # RUF079
            pass
        report(defects)


class Config:
    option = "default"
    for option in options:  # RUF079
        pass
    default = option


def reported_once():
    defects = []
    for defects in batches:  # RUF079
        pass
    report(defects)
    defects = []  # Assignment after the loop shadows the loop variable, not flagged


def temporary_from_earlier_loop_read_after():
    for batch in batches:
        defects = load(batch)
        report(defects)
    for defects in snippets:  # RUF079
        pass
    save(defects)


def used_before_loop_and_read_after():
    defects = get_defects()
    report(defects)
    for defects in snippets:  # RUF079
        pass
    save(defects)


def read_in_loop_else_clause():
    defects = get_defects()
    report(defects)
    for defects in snippets:  # RUF079
        pass
    else:
        save(defects)


def loop_in_later_if_branch(condition):
    defects = get_defects()
    if condition:
        for defects in snippets:  # RUF079
            pass
    save(defects)


def loop_in_later_with_block():
    defects = get_defects()
    with lock:
        for defects in snippets:  # RUF079
            pass
    save(defects)


def loop_in_later_try_block():
    defects = get_defects()
    try:
        for defects in snippets:  # RUF079
            pass
    except ValueError:
        pass
    save(defects)


def loop_header_reads_shadowed_value():
    xs = get_xs()
    report(xs)
    for name, xs in zip(names, xs):  # RUF079
        pass


def loop_header_reads_augmented_value():
    xs = get_xs()
    xs += [extra]
    for name, xs in zip(names, xs):  # RUF079
        pass


### Non-errors

def temporary_from_earlier_loop():
    threads = []
    for _ in range(5):
        thread = Thread(target=work)
        threads.append(thread)
    for thread in threads:
        thread.join()


def temporary_from_earlier_loop_unrelated():
    for event in events:
        key = event.key
        record(key)
    for key, value in mapping.items():
        report(key, value)


def temporary_from_earlier_loop_nested_block():
    for event in events:
        if event.enabled:
            key = event.key
            record(key)
    for key in keys:
        report(key)


def temporary_from_earlier_while_loop():
    while pending:
        item = pending.pop()
        process(item)
    for item in items:
        report(item)


def loop_else_clause_without_read():
    defects = get_defects()
    report(defects)
    for defects in snippets:
        if defects.critical:
            break
    else:
        report_clean()


def used_before_loop():
    fig, ax = subplots()
    ax.plot(xs, ys)
    for ax in axes:
        ax.grid()


def used_before_loop_then_reassigned_after():
    response = complete(prompt)
    log(response)
    for response in responses:
        log(response)
    response = None
    return response


def never_read_before_loop():
    result = []
    for result in results:
        report(result)


def walrus_never_read():
    if (match := find()) is None:
        return
    for match in matches:
        pass


def walrus_used_before_loop():
    if (match := find()) is not None:
        report(match)
    for match in matches:
        pass


def walrus_used_in_comprehension():
    found = [match for key in keys if (match := lookup(key)) is not None]
    for match in found:
        report(match)


def used_only_in_nested_function():
    metrics = ["coverage", "lines"]

    def params():
        return ",".join(metrics)

    fetch(params())
    for metrics in results:
        report(metrics)


def plain_reassignment():
    value = 1
    value = value + 1


def consecutive_loops():
    for i in range(10):
        pass
    for i in range(20):
        pass


def bare_annotation():
    name: str
    for name in names:
        pass


def assignment_after_loop():
    for value in values:
        pass
    value = 0


def reassigned_in_loop_body():
    for line in lines:
        line = line.strip()


def different_branches(condition):
    if condition:
        defects = []
    else:
        for defects in batches:
            pass


def different_try_branches():
    try:
        result = compute()
    except ValueError:
        for result in fallbacks:
            pass


def different_match_cases(value):
    match value:
        case 1:
            result = compute()
        case _:
            for result in fallbacks:
                pass
    return result


def loop_in_later_branch_not_read_after(condition):
    defects = get_defects()
    report(defects)
    if condition:
        for defects in snippets:
            pass


def assignment_in_earlier_branch(condition):
    # The loop doesn't necessarily overwrite anything: `defects` may never have been assigned.
    if condition:
        defects = get_defects()
    for defects in snippets:
        pass
    save(defects)


def deleted_before_loop():
    defects = []
    del defects
    for defects in batches:
        pass


def dummy_variable():
    _ = compute()
    for _ in range(10):
        pass


def parameter(defects):
    # Covered by `redefined-argument-from-local` (PLR1704).
    for defects in batches:
        pass


def function_definition():
    def key():
        pass

    for key in keys:
        pass
    return key


import os

for os in systems:  # Covered by `import-shadowed-by-loop-var` (F402).
    pass


def comprehension():
    item = 1
    values = [item for item in items]


def different_scopes():
    defects = []

    def inner():
        for defects in batches:
            pass


def nested_with_and_loop():
    with open("file.txt") as handle:
        pass
    for handle in handles:
        pass
