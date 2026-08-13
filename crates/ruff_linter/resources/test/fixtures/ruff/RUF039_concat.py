import re


re.compile(
    'implicit'
    'concatenation'
)
re.findall(
    r'''
    multiline
    '''
    """
    concatenation
    """
)
re.finditer(
    f'(?P<{group}>Dynamic'
    r'\s+group'
    'name)'
)
re.fullmatch(
    u'n'r'''eadable'''
    f'much?'
)
re.match(
    b'reak'
    br'eak'
)
re.search(
    r''u''
    '''okay?'''
)
re.split(''U"""w"""U'')
re.sub(
    "I''m o"
    'utta ideas'
)
re.subn("()"r' am I'"??")


import regex


regex.compile(
    'implicit'
    'concatenation'
)
regex.findall(
    r'''
    multiline
    '''
    """
    concatenation
    """
)
regex.finditer(
    f'(?P<{group}>Dynamic'
    r'\s+group'
    'name)'
)
regex.fullmatch(
    u'n'r'''eadable'''
    f'much?'
)
regex.match(
    b'reak'
    br'eak'
)
regex.search(
    r''u''
    '''okay?'''
)
regex.split(''U"""w"""U'')
regex.sub(
    "I''m o"
    'utta ideas'
)
regex.subn("()"r' am I'"??")


regex.template(
    r'''kitty says'''
    r""r''r""r'aw'r""
)
regex.splititer(
    r'r+r*r?'
)
regex.subf(
    rb"haha"
    br"ust go"
    br''br""br''
)
regex.subfn(br'I\s\nee*d\s[O0o]me\x20\Qoffe\E, ' br'b')


# https://github.com/astral-sh/ruff/issues/16713
re.compile(
    "["
    "\U0001F600-\U0001F64F"  # emoticons
    "\U0001F300-\U0001F5FF"  # symbols & pictographs
    "\U0001F680-\U0001F6FF"  # transport & map symbols
    "\U0001F1E0-\U0001F1FF"  # flags (iOS)
    "\U00002702-\U000027B0"
    "\U000024C2-\U0001F251"
    "\u200d"  # zero width joiner
    "\u200c"  # zero width non-joiner
    "\\u200c" # must not be escaped in a raw string
    "]+",
    flags=re.UNICODE,
)
