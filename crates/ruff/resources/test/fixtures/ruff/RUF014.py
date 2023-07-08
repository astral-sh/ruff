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

# Test case found in the Bokeh repository that trigger a false positive.
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

'''
TODO: because `try` statements aren't handled this triggers a false positive as
the last statement is reached, but the rules thinks it isn't (it doesn't
see/process the break statement).

# Test case found in the Bokeh repository that trigger a false positive.
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
'''
