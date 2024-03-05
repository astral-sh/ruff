val = 2

"always ignore this: {val}"

print("but don't ignore this: {val}")  # RUF027


def simple_cases():
    a = 4
    b = "{a}"  # RUF027
    c = "{a} {b} f'{val}' "  # RUF027


def escaped_string():
    a = 4
    b = "escaped string: {{ brackets surround me }}"  # RUF027


def raw_string():
    a = 4
    b = r"raw string with formatting: {a}"  # RUF027
    c = r"raw string with \backslashes\ and \"escaped quotes\": {a}"  # RUF027


def print_name(name: str):
    a = 4
    print("Hello, {name}!")  # RUF027
    print("The test value we're using today is {a}")  # RUF027


def nested_funcs():
    a = 4
    print(do_nothing(do_nothing("{a}")))  # RUF027


def tripled_quoted():
    a = 4
    c = a
    single_line = """ {a} """  # RUF027
    # RUF027
    multi_line = a = """b { # comment
    c}  d
    """


def single_quoted_multi_line():
    a = 4
    # RUF027
    b = " {\
    a} \
    "


def implicit_concat():
    a = 4
    b = "{a}" "+" "{b}" r" \\ "  # RUF027 for the first part only
    print(f"{a}" "{a}" f"{b}")  # RUF027


def escaped_chars():
    a = 4
    b = "\"not escaped:\" '{a}' \"escaped:\": '{{c}}'"  # RUF027


def method_calls():
    value = {}
    value.method = print_name
    first = "Wendy"
    last = "Appleseed"
    value.method("{first} {last}")  # RUF027

def format_specifiers():
    a = 4
    b = "{a:b} {a:^5}"
