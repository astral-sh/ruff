# Get's formatted
a +   b

# yapf: disable
a + [1, 2, 3, 4, 5  ]
# yapf: enable

# Get's formatted again
a +  b


# yapf: disable
a + [1, 2, 3, 4, 5   ]
# fmt: on

# Get's formatted again
a +  b
