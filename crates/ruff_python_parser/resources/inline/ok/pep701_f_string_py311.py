# parse_options: {"target-version": "3.11"}
f"outer {'# not a comment'}"
f'outer {x:{"# not a comment"} }'
f"""{f'''{f'{"# not a comment"}'}'''}"""
f"""{f'''# before expression {f'# aro{f"#{1+1}#"}und #'}'''} # after expression"""
f"escape outside of \t {expr}\n"
f"test\"abcd"
