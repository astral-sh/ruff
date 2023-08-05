def func():
    while False:
        return "unreachable"
    return 1

def func():
    while False:
        return "unreachable"
    else:
        return 1

def func():
    while False:
        return "unreachable"
    else:
        return 1
    return "also unreachable"

def func():
    while True:
        return 1
    return "unreachable"

def func():
    while True:
        return 1
    else:
        return "unreachable"

def func():
    while True:
        return 1
    else:
        return "unreachable"
    return "also unreachable"

def func():
    i = 0
    while False:
        i += 1
    return i

def func():
    i = 0
    while True:
        i += 1
    return i

def func():
    while True:
        pass
    return 1

def func():
    i = 0
    while True:
        if True:
            print("ok")
        i += 1
    return i

def func():
    i = 0
    while True:
        if False:
            print("ok")
        i += 1
    return i

def func():
    while True:
        if True:
            return 1
    return 0

def func():
    while True:
        continue

def func():
    while False:
        continue

def func():
    while True:
        break

def func():
    while False:
        break

def func():
    while True:
        if True:
            continue

def func():
    while True:
        if True:
            break

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
