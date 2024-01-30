val = 2

def simple_cases():
    a = 4
    b = "{a}" # RUF027
    c = "{a} {b} f'{val}' " # RUF027
    uppercase = "{a}".to_upper() # RUF027

def escaped_string():
    a = 4
    b = "escaped string: {{ brackets surround me }}" # RUF027

def raw_string():
    a = 4
    b = r"raw string with formatting: {a}" # RUF027

def print_name(name: str):
    a = 4
    print("Hello, {name}!") # RUF027
    print("The test value we're using today is {a}") # RUF027

def do_nothing(a):
    return a

def do_nothing_with_kwargs(a, **kwargs):
    return a, kwargs

def nested_funcs():
    a = 4
    print(do_nothing(do_nothing("{a}"))) # RUF027
    do_nothing_with_kwargs(do_nothing("{a}"), a = 5) # RUF027


def alternative_formatter(src, **kwargs):
    src.format(**kwargs)

# These should not cause an RUF027 message
def negative_cases():
    a = 4
    positive = False
    """{a}"""
    "don't format: {a}"
    c = """  {b} """
    d = "bad variable: {invalid}"
    e = "incorrect syntax: {}"
    json = "{ positive: false }"
    json2 = "{ 'positive': false }"
    json3 = "{ 'positive': 'false' }"
    alternative_formatter("{a}", a = 5)
    formatted = "{a}".fmt(a = 7)
    print(do_nothing("{a}".format(a=3)))
    print(do_nothing(alternative_formatter("{a}", a = 5)))

a = 4

"always ignore this: {a}"

print("but don't ignore this: {val}")
