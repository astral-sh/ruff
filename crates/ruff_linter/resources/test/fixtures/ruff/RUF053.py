from typing import Generic, ParamSpec, TypeVar, TypeVarTuple, Unpack


_A = TypeVar('_A')
_B = TypeVar('_B', bound=int)
_C = TypeVar('_C', str, bytes)
_D = TypeVar('_D', default=int)
_E = TypeVar('_E', bound=int, default=int)
_F = TypeVar('_F', str, bytes, default=str)
_G = TypeVar('_G', str, a := int)

_As = TypeVarTuple('_As')
_Bs = TypeVarTuple('_Bs', bound=tuple[int, str])
_Cs = TypeVarTuple('_Cs', default=tuple[int, str])

_P1 = ParamSpec('_P1')
_P2 = ParamSpec('_P2', bound=[bytes, bool])
_P3 = ParamSpec('_P3', default=[int, str])


### Errors

class C[T](Generic[_A]): ...
class C[T](Generic[_B], str): ...
class C[T](int, Generic[_C]): ...
class C[T](bytes, Generic[_D], bool): ...  # TODO: Type parameter defaults
class C[T](Generic[_E], list[_E]): ...  # TODO: Type parameter defaults
class C[T](list[_F], Generic[_F]): ...  # TODO: Type parameter defaults

class C[*Ts](Generic[*_As]): ...
class C[*Ts](Generic[Unpack[_As]]): ...
class C[*Ts](Generic[Unpack[_Bs]], tuple[*Bs]): ...
class C[*Ts](Callable[[*_Cs], tuple[*Ts]], Generic[_Cs]): ...  # TODO: Type parameter defaults


class C[**P](Generic[_P1]): ...
class C[**P](Generic[_P2]): ...
class C[**P](Generic[_P3]): ...  # TODO: Type parameter defaults


class C[T](Generic[T, _A]): ...


# See `is_existing_param_of_same_class`.
# `expr_name_to_type_var` doesn't handle named expressions,
# only simple assignments, so there is no fix.
class C[T: (_Z := TypeVar('_Z'))](Generic[_Z]): ...


class C(Generic[_B]):
    class D[T](Generic[_B, T]): ...


class C[T]:
    class D[U](Generic[T, U]): ...


# In a single run, only the first is reported.
# Others will be reported/fixed in following iterations.
class C[T](Generic[_C], Generic[_D]): ...
class C[T, _C: (str, bytes)](Generic[_D]): ...  # TODO: Type parameter defaults


class C[
T  # Comment
](Generic[_E]): ...  # TODO: Type parameter defaults


class C[T](Generic[Generic[_F]]): ...
class C[T](Generic[Unpack[_A]]): ...
class C[T](Generic[Unpack[_P1]]): ...
class C[T](Generic[Unpack[Unpack[_P2]]]): ...
class C[T](Generic[Unpack[*_As]]): ...
class C[T](Generic[Unpack[_As, _Bs]]): ...


class C[T](Generic[_A, _A]): ...
class C[T](Generic[_A, Unpack[_As]]): ...
class C[T](Generic[*_As, _A]): ...


from somewhere import APublicTypeVar
class C[T](Generic[APublicTypeVar]): ...
class C[T](Generic[APublicTypeVar, _A]): ...


# `_G` has two constraints: `str` and `a := int`.
# The latter cannot be used as a PEP 695 constraint,
# as named expressions are forbidden within type parameter lists.
# See also the `_Z` example above.
class C[T](Generic[_G]): ...  # Should be moved down below eventually


# Single-element constraints should not be converted to a bound.
class C[T: (str,)](Generic[_A]): ...
class C[T: [a]](Generic[_A]): ...


# Existing bounds should not be deparenthesized.
# class C[T: (_Y := int)](Generic[_A]): ...  # TODO: Uncomment this
# class C[T: (*a,)](Generic[_A]): ...        # TODO: Uncomment this


### No errors

class C(Generic[_A]): ...
class C[_A]: ...
class C[_A](list[_A]): ...
class C[_A](list[Generic[_A]]): ...
