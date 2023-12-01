def with_backslash():
    """Sum\\mary."""


def ends_in_quote():
    'Sum\\mary."'


def contains_quote():
    'Sum"\\mary.'


# OK
def contains_triples(t):
    """('''|\""")"""


# OK
def contains_triples(t):
    '''(\'''|""")'''


# TODO: here should raise D300 for using dobule quotes instead,
# because escaped double quote does allow us.
def contains_triples(t):
    '''(\""")'''
