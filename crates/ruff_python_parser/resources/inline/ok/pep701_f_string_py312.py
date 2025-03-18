# parse_options: {"target-version": "3.12"}
f'Magic wand: { bag['wand'] }'     # nested quotes
f"{'\n'.join(a)}"                  # escape sequence
f'''A complex trick: {
    bag['bag']                     # comment
}'''
f"{f"{f"{f"{f"{f"{1+1}"}"}"}"}"}"  # arbitrary nesting
f"{f'''{"nested"} inner'''} outer" # nested (triple) quotes
f"test {a \
    } more"                        # line continuation
