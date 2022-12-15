# Replace names by built-in names, whether namespaced or not
# https://github.com/search?q=%22from+six+import%22&type=code
import six
from six.moves import map  # No need
from six import text_type

six.text_type  # str
six.binary_type  # bytes
six.class_types  # (type,)
six.string_types  # (str,)
six.integer_types  # (int,)
six.unichr  # chr
six.iterbytes  # iter
six.print_(...)  # print(...)
six.exec_(c, g, l)  # exec(c, g, l)
six.advance_iterator(it)  # next(it)
six.next(it)  # next(it)
six.callable(x)  # callable(x)
six.moves.range(x)  # range(x)
six.moves.xrange(x)  # range(x)
isinstance(..., six.class_types)  # isinstance(..., type)
issubclass(..., six.integer_types)  # issubclass(..., int)
isinstance(..., six.string_types)  # isinstance(..., str)

# Replace call on arg by method call on arg
six.iteritems(dct)  # dct.items()
six.iterkeys(dct)  # dct.keys()
six.itervalues(dct)  # dct.values()
six.viewitems(dct)  # dct.items()
six.viewkeys(dct)  # dct.keys()
six.viewvalues(dct)  # dct.values()
six.assertCountEqual(self, a1, a2)  # self.assertCountEqual(a1, a2)
six.assertRaisesRegex(self, e, r, fn)  # self.assertRaisesRegex(e, r, fn)
six.assertRegex(self, s, r)  # self.assertRegex(s, r)

# Replace call on arg by arg attribute
six.get_method_function(meth)  # meth.__func__
six.get_method_self(meth)  # meth.__self__
six.get_function_closure(fn)  # fn.__closure__
six.get_function_code(fn)  # fn.__code__
six.get_function_defaults(fn)  # fn.__defaults__
six.get_function_globals(fn)  # fn.__globals__

# Replace by string literal
six.b("...")  # b'...'
six.u("...")  # '...'
six.ensure_binary("...")  # b'...'
six.ensure_str("...")  # '...'
six.ensure_text("...")  # '...'
six.b(string)  # no change

# Replace by simple expression
six.get_unbound_function(meth)  # meth
six.create_unbound_method(fn, cls)  # fn

# Raise exception
six.raise_from(exc, exc_from)  # raise exc from exc_from
six.reraise(tp, exc, tb)  # raise exc.with_traceback(tb)
six.reraise(*sys.exc_info())  # raise

# Int / Bytes conversion
six.byte2int(bs)  # bs[0]
six.indexbytes(bs, i)  # bs[i]
six.int2byte(i)  # bytes((i, ))

# Special cases for next calls
next(six.iteritems(dct))  # next(iter(dct.items()))
next(six.iterkeys(dct))  # next(iter(dct.keys()))
next(six.itervalues(dct))  # next(iter(dct.values()))

# TODO: To implement


# Rewrite classes
@six.python_2_unicode_compatible  # Remove
class C(six.Iterator):
    pass  # class C: pass


class C(six.with_metaclass(M, B)):
    pass  # class C(B, metaclass=M): pass


# class C(B, metaclass=M): pass
@six.add_metaclass(M)
class C(B):
    pass
