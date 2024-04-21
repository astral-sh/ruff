def after_return():
    return "reachable"
    return "unreachable"

async def also_works_on_async_functions():
    return "reachable"
    return "unreachable"

def if_always_true():
    if True:
        return "reachable"
    return "unreachable"

def if_always_false():
    if False:
        return "unreachable"
    return "reachable"

def if_elif_always_false():
    if False:
        return "unreachable"
    elif False:
        return "also unreachable"
    return "reachable"

def if_elif_always_true():
    if False:
        return "unreachable"
    elif True:
        return "reachable"
    return "also unreachable"

def ends_with_if():
    if False:
        return "unreachable"
    else:
        return "reachable"

def infinite_loop():
    while True:
        continue
    return "unreachable"

'''  TODO: we could determine these, but we don't yet.
def for_range_return():
    for i in range(10):
        if i == 5:
            return "reachable"
    return "unreachable"

def for_range_else():
    for i in range(111):
        if i == 5:
            return "reachable"
    else:
        return "unreachable"
    return "also unreachable"

def for_range_break():
    for i in range(13):
        return "reachable"
    return "unreachable"

def for_range_if_break():
    for i in range(1110):
        if True:
            return "reachable"
    return "unreachable"
'''

def match_wildcard(status):
    match status:
        case _:
            return "reachable"
    return "unreachable"

def match_case_and_wildcard(status):
    match status:
        case 1:
            return "reachable"
        case _:
            return "reachable"
    return "unreachable"

def raise_exception():
    raise Exception
    return "unreachable"

def while_false():
    while False:
        return "unreachable"
    return "reachable"

def while_false_else():
    while False:
        return "unreachable"
    else:
        return "reachable"

def while_false_else_return():
    while False:
        return "unreachable"
    else:
        return "reachable"
    return "also unreachable"

def while_true():
    while True:
        return "reachable"
    return "unreachable"

def while_true_else():
    while True:
        return "reachable"
    else:
        return "unreachable"

def while_true_else_return():
    while True:
        return "reachable"
    else:
        return "unreachable"
    return "also unreachable"

def while_false_var_i():
    i = 0
    while False:
        i += 1
    return i

def while_true_var_i():
    i = 0
    while True:
        i += 1
    return i

def while_infinite():
    while True:
        pass
    return "unreachable"

def while_if_true():
    while True:
        if True:
            return "reachable"
    return "unreachable"

def while_break():
    while True:
        print("reachable")
        break
        print("unreachable")
    return "reachable"

# Test case found in the Bokeh repository that triggered a false positive.
def bokeh1(self, obj: BytesRep) -> bytes:
    data = obj["data"]

    if isinstance(data, str):
        return base64.b64decode(data)
    elif isinstance(data, Buffer):
        buffer = data
    else:
        id = data["id"]

        if id in self._buffers:
            buffer = self._buffers[id]
        else:
            self.error(f"can't resolve buffer '{id}'")

    return buffer.data

# Test case found in the Bokeh repository that triggered a false positive.
def bokeh2(self, host: str = DEFAULT_HOST, port: int = DEFAULT_PORT) -> None:
    self.stop_serving = False
    while True:
        try:
            self.server = HTTPServer((host, port), HtmlOnlyHandler)
            self.host = host
            self.port = port
            break
        except OSError:
            log.debug(f"port {port} is in use, trying to next one")
            port += 1

    self.thread = threading.Thread(target=self._run_web_server)

# Test case found in the pandas repository that triggered a false positive.
def _check_basic_constructor(self, empty):
    # mat: 2d matrix with shape (3, 2) to input. empty - makes sized
    # objects
    mat = empty((2, 3), dtype=float)
    # 2-D input
    frame = DataFrame(mat, columns=["A", "B", "C"], index=[1, 2])

    assert len(frame.index) == 2
    assert len(frame.columns) == 3

    # 1-D input
    frame = DataFrame(empty((3,)), columns=["A"], index=[1, 2, 3])
    assert len(frame.index) == 3
    assert len(frame.columns) == 1

    if empty is not np.ones:
        msg = r"Cannot convert non-finite values \(NA or inf\) to integer"
        with pytest.raises(IntCastingNaNError, match=msg):
            DataFrame(mat, columns=["A", "B", "C"], index=[1, 2], dtype=np.int64)
        return
    else:
        frame = DataFrame(
            mat, columns=["A", "B", "C"], index=[1, 2], dtype=np.int64
        )
        assert frame.values.dtype == np.int64

    # wrong size axis labels
    msg = r"Shape of passed values is \(2, 3\), indices imply \(1, 3\)"
    with pytest.raises(ValueError, match=msg):
        DataFrame(mat, columns=["A", "B", "C"], index=[1])
    msg = r"Shape of passed values is \(2, 3\), indices imply \(2, 2\)"
    with pytest.raises(ValueError, match=msg):
        DataFrame(mat, columns=["A", "B"], index=[1, 2])

    # higher dim raise exception
    with pytest.raises(ValueError, match="Must pass 2-d input"):
        DataFrame(empty((3, 3, 3)), columns=["A", "B", "C"], index=[1])

    # automatic labeling
    frame = DataFrame(mat)
    tm.assert_index_equal(frame.index, Index(range(2)), exact=True)
    tm.assert_index_equal(frame.columns, Index(range(3)), exact=True)

    frame = DataFrame(mat, index=[1, 2])
    tm.assert_index_equal(frame.columns, Index(range(3)), exact=True)

    frame = DataFrame(mat, columns=["A", "B", "C"])
    tm.assert_index_equal(frame.index, Index(range(2)), exact=True)

    # 0-length axis
    frame = DataFrame(empty((0, 3)))
    assert len(frame.index) == 0

    frame = DataFrame(empty((3, 0)))
    assert len(frame.columns) == 0
