d = {}

### Errors

d.update({'a': 'b'})                      # Strings
d.update({a: b})                          # Identifiers

d.update({                                # Multiline
                                          # expressions
    1 + 2: lorem() * ~ipsum() @ dolor()
})


# No errors
d.update({})                              # No items
d.update({1: 2, 3: 4})                    # Multiple items
d.update({b'': b'', **{}})                # Inner unpack
d.update(**{D(): E().foo()})              # Unpacked argument
d.update({1_2.3_4: {}}, **{})             # Second unpacked argument
d.update({{}: ()}, foo="bar")             # Keyword arguments
