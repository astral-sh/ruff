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


def nested_target():
    item = None
    for idx, (key, item) in enumerate(pairs):  # RUF079
        pass


async def async_loop():
    result = []
    async for result in stream():  # RUF079
        pass


def with_target():
    handle = open_default()
    with open("file.txt") as handle:  # RUF079
        pass
    return handle


def with_tuple_target():
    reader = None
    with pair() as (reader, writer):  # RUF079
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


def annotated_assignment():
    count: int = 0
    for count in counts:  # RUF079
        pass


def shadowed_inside_loop_body():
    for batch in batches:
        defects = []
        for defects in batch:  # RUF079
            pass


class Config:
    option = "default"
    for option in options:  # RUF079
        pass


def reported_once():
    defects = []
    for defects in batches:  # RUF079
        pass
    defects = []  # Assignment after the loop shadows the loop variable, not flagged


### Non-errors

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
