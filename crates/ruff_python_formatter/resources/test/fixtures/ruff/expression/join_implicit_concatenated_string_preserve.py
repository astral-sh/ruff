a = "different '" 'quote "are fine"'  # join

# More single quotes
"one single'" "two 'single'" ' two "double"'

# More double quotes
'one double"' 'two "double"' " two 'single'"

# Equal number of single and double quotes
'two "double"' " two 'single'"

# Already invalid Pre Python 312
f"{'Hy "User"'}" f'{"Hy 'User'"}'
