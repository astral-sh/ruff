this_should_raise_Q003 = 'This is a \'string\''
this_should_raise_Q003 = 'This is \\ a \\\'string\''
this_is_fine = '"This" is a \'string\''
this_is_fine = "This is a 'string'"
this_is_fine = "\"This\" is a 'string'"
this_is_fine = r'This is a \'string\''
this_is_fine = R'This is a \'string\''
this_should_raise = (
    'This is a'
    '\'string\''
)
