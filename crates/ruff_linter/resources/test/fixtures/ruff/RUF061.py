# Errors
t"some user {inputs}"f"and more {unsafe}"
f"some user {inputs}"t"and more {unsafe}"
t"""multiline {stuff}"""\
f"""that's {dangerous}"""
t"this {isalso}" f"still not the best even without exprs here"

# Ok
t"this is {f"{fine}"}"
t"and {so}" "is this"
