def foo(items: List[int]):
    for item in list(items):  # PERF101
        pass


def bar(items: Dict[str, Any]):
    for item in list(items):  # Ok
        pass


a = [1,2,3]
foo(a)
