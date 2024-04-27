"""Test type parameters and aliases"""

# Type parameters in type alias statements

from some_module import Bar

type Foo[T] = T # OK
type Foo[T] = list[T] # OK
type Foo[T: ForwardA] = T # OK
type Foo[*Ts] = Bar[Ts] # OK
type Foo[**P] = Bar[P] # OK
class ForwardA: ...

# Types used in aliased assignment must exist

type Foo = DoesNotExist  # F821: Undefined name `DoesNotExist`
type Foo = list[DoesNotExist]  # F821: Undefined name `DoesNotExist`

# Type parameters do not escape alias scopes

type Foo[T] = T
T  # F821: Undefined name `T` - not accessible afterward alias scope

# Type parameters in functions

def foo[T](t: T) -> T: return t  # OK
async def afoo[T](t: T) -> T: return t  # OK
def with_forward_ref[T: ForwardB](t: T) -> T: return t  # OK
def can_access_inside[T](t: T) -> T:  # OK
    print(T)  # OK
    return t  # OK
class ForwardB: ...


# Type parameters do not escape function scopes

from some_library import some_decorator

@some_decorator(T)  # F821: Undefined name `T` - not accessible in decorators

def foo[T](t: T) -> None: ...
T  # F821: Undefined name `T` - not accessible afterward function scope


# Type parameters in classes

class Foo[T](list[T]): ... # OK
class UsesForward[T: ForwardC](list[T]): ...    # OK
class ForwardC: ...
class WithinBody[T](list[T]):   # OK
    t = T   # OK
    x: T  # OK

    def foo(self, x: T) -> T:  # OK
        return x

    def foo(self):
        T  # OK


# Type parameters do not escape class scopes

from some_library import some_decorator
@some_decorator(T)  # F821: Undefined name `T` - not accessible in decorators

class Foo[T](list[T]): ...
T  # F821: Undefined name `T` - not accessible after class scope

# Types specified in bounds should exist

type Foo[T: DoesNotExist] = T  # F821: Undefined name `DoesNotExist`
def foo[T: DoesNotExist](t: T) -> T: return t  # F821: Undefined name `DoesNotExist`
class Foo[T: DoesNotExist](list[T]): ...  # F821: Undefined name `DoesNotExist`

type Foo[T: (DoesNotExist1, DoesNotExist2)] = T  # F821: Undefined name `DoesNotExist1`, Undefined name `DoesNotExist2`
def foo[T: (DoesNotExist1, DoesNotExist2)](t: T) -> T: return t  # F821: Undefined name `DoesNotExist1`, Undefined name `DoesNotExist2`
class Foo[T: (DoesNotExist1, DoesNotExist2)](list[T]): ...  # F821: Undefined name `DoesNotExist1`, Undefined name `DoesNotExist2`

# Same in defaults

type Foo[T = DoesNotExist] = T  # F821: Undefined name `DoesNotExist`
def foo[T = DoesNotExist](t: T) -> T: return t  # F821: Undefined name `DoesNotExist`
class Foo[T = DoesNotExist](list[T]): ...  # F821: Undefined name `DoesNotExist`

type Foo[T = (DoesNotExist1, DoesNotExist2)] = T  # F821: Undefined name `DoesNotExist1`, Undefined name `DoesNotExist2`
def foo[T = (DoesNotExist1, DoesNotExist2)](t: T) -> T: return t  # F821: Undefined name `DoesNotExist1`, Undefined name `DoesNotExist2`
class Foo[T = (DoesNotExist1, DoesNotExist2)](list[T]): ...  # F821: Undefined name `DoesNotExist1`, Undefined name `DoesNotExist2`

# Type parameters in nested classes

class Parent[T]:
    t = T   # OK

    def can_use_class_variable(self, x: t) -> t:  # OK
        return x

    class Child:
        def can_access_parent_type_parameter(self, x: T) -> T:  # OK
            T  # OK
            return x

        def cannot_access_parent_variable(self, x: t) -> t:  # F821: Undefined name `T`
                t # F821: Undefined name `t`
                return x

# Type parameters in nested functions

def can_access_inside_nested[T](t: T) -> T:  # OK
    def bar(x: T) -> T:  # OK
        T # OK
        return x

    bar(t)
