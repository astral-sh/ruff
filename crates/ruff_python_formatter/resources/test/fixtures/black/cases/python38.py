def starred_return():
    my_list = ["value2", "value3"]
    return "value1", *my_list


def starred_yield():
    my_list = ["value2", "value3"]
    yield "value1", *my_list


# all right hand side expressions allowed in regular assignments are now also allowed in
# annotated assignments
a : Tuple[ str, int] = "1", 2
a: Tuple[int , ... ] = b,  *c, d
def t():
    a : str =  yield "a"
