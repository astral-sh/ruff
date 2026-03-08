# Bad

function(arg1,
         arg2=[{f"""
                data{i}
                """} for i in range(5)])  # line indent = 16, not matching the previous lines

# Good

function(
    arg1,
    arg2=[{f"""
        data{i}
    """} for i in range(5)],  # line indent = 4, matching the line with opening brackets
)  # line indent = 0, matching the line with opening parenthesis
