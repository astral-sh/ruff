# parse_options: {"target-version": "3.11"}
f"outer {'# not a comment'}"
f'outer {x:{"# not a comment"} }'
f"""{f'''{f'{"# not a comment"}'}'''}"""
f"""{f'''# before expression {f'# aro{f"#{1+1}#"}und #'}'''} # after expression"""
f"escape outside of \t {expr}\n"
f"test\"abcd"
f"{1:\x64}"  # escapes are valid in the format spec
f"{1:\"d\"}"  # this also means that escaped outer quotes are valid
