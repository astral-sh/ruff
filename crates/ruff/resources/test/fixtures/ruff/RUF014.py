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
