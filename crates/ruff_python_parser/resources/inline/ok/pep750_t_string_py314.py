# parse_options: {"target-version": "3.14"}
t'Magic wand: { bag['wand'] }'     # nested quotes
t"{'\n'.join(a)}"                  # escape sequence
t'''A complex trick: {
    bag['bag']                     # comment
}'''
t"{t"{t"{t"{t"{t"{1+1}"}"}"}"}"}"  # arbitrary nesting
t"{t'''{"nested"} inner'''} outer" # nested (triple) quotes
t"test {a \
    } more"                        # line continuation
