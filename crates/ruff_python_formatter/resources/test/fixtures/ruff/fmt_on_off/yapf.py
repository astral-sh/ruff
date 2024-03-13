# Gets formatted
a +   b

# yapf: disable
a + [1, 2, 3, 4, 5  ]
# yapf: enable

# Gets formatted again
a +  b


# yapf: disable
a + [1, 2, 3, 4, 5   ]
# fmt: on

# Gets formatted again
a +  b
