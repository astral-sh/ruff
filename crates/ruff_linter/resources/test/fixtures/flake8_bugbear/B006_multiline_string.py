def multiline_string_default(
    value=[
        """first line
        second line"""
    ]
):
    return value


def multiline_fstring_default(
    value=[
        f"""first {1}
        second {2}"""
    ]
):
    return value
