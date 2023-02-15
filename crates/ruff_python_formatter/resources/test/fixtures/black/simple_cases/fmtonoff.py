#!/usr/bin/env python3
import asyncio
import sys

from third_party import X, Y, Z

from library import some_connection, \
                    some_decorator
# fmt: off
from third_party import (X,
                         Y, Z)
# fmt: on
f'trigger 3.6 mode'
# Comment 1

# Comment 2

# fmt: off
def func_no_args():
  a; b; c
  if True: raise RuntimeError
  if False: ...
  for i in range(10):
    print(i)
    continue
  exec('new-style exec', {}, {})
  return None
async def coroutine(arg, exec=False):
 'Single-line docstring. Multiline is harder to reformat.'
 async with some_connection() as conn:
     await conn.do_what_i_mean('SELECT bobby, tables FROM xkcd', timeout=2)
 await asyncio.sleep(1)
@asyncio.coroutine
@some_decorator(
with_args=True,
many_args=[1,2,3]
)
def function_signature_stress_test(number:int,no_annotation=None,text:str='default',* ,debug:bool=False,**kwargs) -> str:
 return text[number:-1]
# fmt: on
def spaces(a=1, b=(), c=[], d={}, e=True, f=-1, g=1 if False else 2, h="", i=r''):
 offset = attr.ib(default=attr.Factory( lambda: _r.uniform(1, 2)))
 assert task._cancel_stack[:len(old_stack)] == old_stack
def spaces_types(a: int = 1, b: tuple = (), c: list = [], d: dict = {}, e: bool = True, f: int = -1, g: int = 1 if False else 2, h: str = "", i: str = r''): ...
def spaces2(result= _core.Value(None)):
 ...
something = {
    # fmt: off
    key: 'value',
}

def subscriptlist():
    atom[
        # fmt: off
        'some big and',
        'complex subscript',
        # fmt: on
        goes + here, andhere,
       ]

def import_as_names():
    # fmt: off
    from hello import a,        b
    'unformatted'
    # fmt: on

def testlist_star_expr():
    # fmt: off
    a , b = *hello
    'unformatted'
    # fmt: on

def yield_expr():
    # fmt: off
    yield hello
    'unformatted'
    # fmt: on
    'formatted'
    # fmt: off
    ( yield hello )
    'unformatted'
    # fmt: on

def example(session):
    # fmt: off
    result = session\
        .query(models.Customer.id)\
        .filter(models.Customer.account_id == account_id,
                models.Customer.email == email_address)\
        .order_by(models.Customer.id.asc())\
        .all()
    # fmt: on
def off_and_on_without_data():
    """All comments here are technically on the same prefix.

    The comments between will be formatted. This is a known limitation.
    """
    # fmt: off


        #hey, that won't work


    # fmt: on
    pass
def on_and_off_broken():
    """Another known limitation."""
    # fmt: on
    # fmt: off
    this=should.not_be.formatted()
    and_=indeed . it  is  not  formatted
    because . the . handling . inside . generate_ignored_nodes()
    now . considers . multiple . fmt . directives . within . one . prefix
    # fmt: on
    # fmt: off
        # ...but comments still get reformatted even though they should not be
    # fmt: on
def long_lines():
    if True:
        typedargslist.extend(
            gen_annotated_params(ast_args.kwonlyargs, ast_args.kw_defaults, parameters, implicit_default=True)
        )
        # fmt: off
        a = (
            unnecessary_bracket()
        )
        # fmt: on
    _type_comment_re = re.compile(
        r"""
        ^
        [\t ]*
        \#[ ]type:[ ]*
        (?P<type>
            [^#\t\n]+?
        )
        (?<!ignore)     # note: this will force the non-greedy + in <type> to match
                        # a trailing space which is why we need the silliness below
        (?<!ignore[ ]{1})(?<!ignore[ ]{2})(?<!ignore[ ]{3})(?<!ignore[ ]{4})
        (?<!ignore[ ]{5})(?<!ignore[ ]{6})(?<!ignore[ ]{7})(?<!ignore[ ]{8})
        (?<!ignore[ ]{9})(?<!ignore[ ]{10})
        [\t ]*
        (?P<nl>
            (?:\#[^\n]*)?
            \n?
        )
        $
        """,
        # fmt: off
        re.MULTILINE|re.VERBOSE
        # fmt: on
    )
def single_literal_yapf_disable():
    """Black does not support this."""
    BAZ = {
        (1, 2, 3, 4),
        (5, 6, 7, 8),
        (9, 10, 11, 12)
    }  # yapf: disable
cfg.rule(
    "Default", "address",
    xxxx_xxxx=["xxx-xxxxxx-xxxxxxxxxx"],
    xxxxxx="xx_xxxxx", xxxxxxx="xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    xxxxxxxxx_xxxx=True, xxxxxxxx_xxxxxxxxxx=False,
    xxxxxx_xxxxxx=2, xxxxxx_xxxxx_xxxxxxxx=70, xxxxxx_xxxxxx_xxxxx=True,
    # fmt: off
    xxxxxxx_xxxxxxxxxxxx={
        "xxxxxxxx": {
            "xxxxxx": False,
            "xxxxxxx": False,
            "xxxx_xxxxxx": "xxxxx",
        },
        "xxxxxxxx-xxxxx": {
            "xxxxxx": False,
            "xxxxxxx": True,
            "xxxx_xxxxxx": "xxxxxx",
        },
    },
    # fmt: on
    xxxxxxxxxx_xxxxxxxxxxx_xxxxxxx_xxxxxxxxx=5
)
# fmt: off
yield  'hello'
# No formatting to the end of the file
l=[1,2,3]
d={'a':1,
   'b':2}
