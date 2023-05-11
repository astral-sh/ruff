bla = b"bla"

def foo(one_arg):
    pass

f"Error: {str(bla)}, {repr(bla)}, {ascii(bla)}"

f"Ok: {foo(bla)}"

f"Ok: {str(bla, 'ascii')}, {str(bla, encoding='cp1255')}"

f"Ok: {bla!s} {[]!r} {'bar'!a}"

"Ok: Not an f-string {str(bla)}, {repr(bla)}, {ascii(bla)}"