# Recursive types in ty

**15-minute walkthrough of [PR #28425](https://github.com/astral-sh/ruff/pull/28425)**
PR by [mtshiba](https://github.com/mtshiba), presentation by [carljm](https://github.com/carljm).

Recursive implicit aliases now keep enough structure to correctly and precisely check values at
every nesting level.

______________________________________________________________________

## 1. The problem: checking recursively nested values

A recursive alias describes a shape that can repeat arbitrarily many times:

```python
Tree = int | tuple["Tree"]

def valid_tree() -> Tree:
    return (((1,),),)

def invalid_tree() -> Tree:
    return ((("leaf",),),)  # error: invalid-return-type
```

Each value is either an `int` or a one-element tuple containing another `Tree`. Checking only the
outermost tuple cannot distinguish these returns. We have to preserve the rule an arbitrary number
of levels, all the way to the leaf.

This is the **meaning of the alias**. Next we'll look at how ty stores it.

[Regression test: nested values][nested-test]

______________________________________________________________________

## 2. The old representation: the Divergent type

Let's inspect the same `Tree` type:

```python
from typing import reveal_type

Tree = int | tuple["Tree"]

def inspect(value: Tree):
    reveal_type(value)
    # Before: int | tuple[Divergent]
    # After:  Tree
```

Previously, we broke the cycle with `Divergent`, a gradual approximation that permits all
assignments and loses the recursive shape.

After narrowing to the tuple alternative, the old subscripting behavior shows the loss of information:

```text
value        : tuple[Divergent]
value[0]     : Divergent
```

With the `Divergent` approximation, we no longer require the child to be an `int` or another
one-element tuple, so the wrong `"leaf"` return from above can wrongly pass checking. With this PR,
each nested tuple element is another `Tree`:

```python
def inspect_children(value: Tree):
    if isinstance(value, tuple):
        reveal_type(value)  # tuple[Tree]
        child = value[0]
        reveal_type(child)  # Tree
        if isinstance(child, tuple):
            reveal_type(child[0])  # Tree
```

[Nested-value regression test][nested-test] · [Subscripting implementation][subscript-source]

______________________________________________________________________

## 3. The new representation: a body and a bound reference

We now model that `Tree` alias via "mu types", named for the lowercase Greek letter μ, which is
often used to spell such types:

```text
Tree = μr. (int | tuple[r])
```

Read `μr. ...` as “bind `r` to this whole recursive type.” The binder gives meaning to the
occurrence of `r` within the body. Where we see `r` in the body, we can substitute the entire
recursive type `μr. (int | tuple[r])`.

The type `int | tuple[r]` on its own is **open**, because it contains a free (unbound) recursive
variable `r`.

Enclosing it in its binder (`μr.`) makes it **closed**: now there are no free recursive variables,
all are bound.

An open type is not semantically meaningful; a closed one is. So a key invariant of this
implementation is that **semantic type operations can only ever see closed types**.

Let's look at the Rust-level implementation. We have two new `Type` variants, `Type::Recursive`
and `Type::RecursiveVar`.

`Type::Recursive` carries a `RecursiveType` that stores the binder, `μr.`, and its body. The struct
has these fields (plus some others we can ignore for now):

```rust
struct RecursiveType<'db> {
    cycle: RecursiveCycle,
    body: Type<'db>,
}
```

The `cycle` field is just an opaque identity for this particular recursion; we don't need to worry
about its details for now; it's just an identifier, like `r` in the notation above.

The `body` field contains the body of our recursive type, e.g. `int | tuple[r]`. This will be an
open type! Somewhere within it, it will contain a `Type::RecursiveVar`, which just holds the same
cycle ID:

```rust
struct RecursiveVar<'db> {
    cycle: RecursiveCycle,
}
```

This variable refers to the whole recursive type bound by the nearest enclosing `RecursiveType`
with the matching `cycle` ID.

`Type` can represent both open and closed types, so Rust does not enforce this boundary for us.
Semantic operations assert that no free `RecursiveVar` reaches them, which is why you'll see this
arm in almost every exhaustive `Type` match:

```rust
Type::RecursiveVar(_) => {
    unreachable!("semantic operation on an unbound recursive variable")
}
```

[Representation and invariant][recursive-source]

______________________________________________________________________

## 4. Unfolding makes one layer available

To inspect `Tree`, we substitute the whole recursive type for its bound reference. We call this
operation "unfold":

```text
unfold(μr. body) = body[r := μr. body]

unfold(Tree) = int | tuple[Tree]
```

After one unfold, we now have a normal union type with no `RecursiveType` wrapper. One of the
union's elements is a single-element tuple, whose element type is now `Tree`, which is still a
`RecursiveType`. We can use normal union and tuple operations, then unfold again if/when a later
operation needs another layer.

```python
from typing import reveal_type

Tree = int | tuple["Tree"]

def inspect(value: Tree):
    if isinstance(value, tuple):
        reveal_type(value)  # tuple[Tree]
        reveal_type(value[0])  # Tree
    else:
        reveal_type(value)  # int
```

The actual subscript match arm uses `RecursiveType::map_or_else` to unfold once, then call one of
two callbacks. It compares the resulting `Type` value to the original recursive type:

```rust
(Type::Recursive(recursive), _) => Some(recursive.map_or_else(
    db,
    env,
    // Fallback: unfolding returned exactly the original type.
    || Ok(value_ty),
    // Operation: unfolding exposed a different type to inspect.
    |unfolded| unfolded.subscript(db, env, slice_ty, expr_context),
)),
```

For `Tree`, unfolding exposes `int | tuple[Tree]`. The second callback runs the subscript operation
on that union.

The first callback handles the case where unfolding exposes no new structure. During inference,
before the body is known, we use a placeholder whose body is just its own recursive variable:

```text
P = μr. r
unfold(P) = r[r := P] = P
```

Replacing `r` with the whole type gives us exactly `P` again. Calling `subscript` on that result
would repeat the same call forever. Instead, `|| Ok(value_ty)` returns the original placeholder as
the provisional subscript result. It just preserves the recursive reference while inference is still
resolving the cycle.

In general, we ensure the invariant that semantic operations never encounter open types by having
`RecursiveType` match arms always use unfolding operations like `RecursiveType::map_or_else`.

Callers can also request the unfolded type directly:

```rust
let unfolded = recursive.unfold(db, env);
```

This returns a closed `Type`: `int | tuple[Tree]` for `Tree`. It does not run another operation or
choose a fallback. A caller that continues recursively must handle the unchanged result or use a
recursion guard. `map_or_else` builds on `unfold` by checking for that unchanged result and choosing
between the two callbacks.

[Subscript match arm][subscript-source] · [`map_or_else`][map-or-else-source] · [Unfolding][unfold-source]

______________________________________________________________________

## 5. Constructing the recursive type during inference

The new `infer_implicit_alias_type` query infers the right-hand-side of an implicit type alias
directly as a type expression, separately from its runtime value.

If the alias turns out to be recursive, that will cause a query cycle in
`infer_implicit_alias_type`. Salsa's cycle handling creates and then completes the `RecursiveType`
to represent this recursive alias.

For `Tree = int | tuple["Tree"]`, the process looks like this:

```text
1. On a query cycle, return a closed provisional `RecursiveType` from `cycle_initial`:

   P = μr. r

2. Infer the alias body using that provisional type (this is still a closed type):

   int | tuple[P]

3. Recover the cycle in `cycle_fn`: "bind" any RecursiveType for cycle "r", then enclose the body:

   μr. (int | tuple[r])
```

The `RecursiveType::bind` operation in step 3 uses a type mapping. Given a cycle identity `r` and a
type (in this case `int | tuple[P]`, where `P` is a `RecursiveType` for cycle `r`), the mapping finds
matching `RecursiveType` occurrences and replaces each with a `RecursiveVar` for `r`. This produces
the open body `int | tuple[r]`. After checking the result, the `bind` method wraps this body in a
new `RecursiveType` for `r`, producing `μr. (int | tuple[r])`.

The "original" `P` was simply `μr. r`, the provisional cycle-initial type, but after cycle iteration
it may have gained a layer of recursion, so it may be a more complex `RecursiveType`. The "bind"
operation doesn't actually care about its internal structure: as long as it has cycle identity `r`,
"bind" will replace it with a `RecursiveVar` for `r`.

Binding and unfolding go in opposite directions: binding replaces matching recursive types with
variables; unfolding replaces those variables with the whole recursive type.

The substitutions in both operations use `TypeMapping::ApplyRecursiveSubstitution`, which
carries a `RecursiveSubstitution` enum with `Bind` and `Unfold` variants.

[Salsa query][inference-source] · [Initial value and cycle recovery][binding-source]

______________________________________________________________________

## 6. Generic recursion: arguments can change at every step

Now let's make this all more fun: what if a recursive alias is generic?

Let's keep the same alias shape for our example: a leaf or a one-element tuple. We'll make the leaf
type a parameter, and let each tuple layer wrap it in `list`:

```python
from typing import TypeVar, reveal_type

T = TypeVar("T")
Tree = T | tuple["Tree[list[T]]"]

def inspect(tree: Tree[int]):
    if isinstance(tree, tuple):
        child = tree[0]
        reveal_type(child)  # Tree[list[int]]
        if isinstance(child, tuple):
            reveal_type(child[0])  # Tree[list[list[int]]]
```

The allowed leaf is now `int` at the root, `list[int]` inside one tuple, and `list[list[int]]` inside
two tuples:

```python
root: Tree[int] = 1
one_layer: Tree[int] = ([1],)
two_layers: Tree[int] = (([[1]],),)
```

`Tree` is now a "type constructor": applying it to an argument, as in `Tree[int]`, produces a type.
To represent these applications, both structs have an `arguments` field that we omitted earlier,
which holds an optional `Specialization`:

```rust
struct RecursiveType<'db> {
    cycle: RecursiveCycle,
    body: Type<'db>,
    arguments: Option<Specialization<'db>>,
}

struct RecursiveVar<'db> {
    cycle: RecursiveCycle,
    arguments: Option<Specialization<'db>>,
}
```

`Specialization` records a substitution for the alias's type parameters. Both fields were `None`
for our earlier non-generic `Tree`. Here, they record two distinct substitutions (written as argument
lists, so `[int]` means `T := int`):

| Stored on                            | Meaning                                                        |
| ------------------------------------ | -------------------------------------------------------------- |
| `RecursiveType.arguments = [int]`    | This application uses `T = int`.                               |
| `RecursiveVar.arguments = [list[T]]` | The recursive occurrence applies the constructor to `list[T]`. |

The stored body of a `RecursiveType` is kept unspecialized! So for `Tree[int]`, the body is still
just `T | tuple[F[list[T]]]` (where `F[list[T]]` represents a `RecursiveVar` with
`arguments: list[T]`). The fact that we've specialized it to `int` is stored only in the `arguments` of the
outer `RecursiveType`. This avoids an infinitely-growing eager expansion.

We don't actually apply the specialization until we unfold:

```text
Stored body:             T | tuple[F[list[T]]]
1. Close the body:       T | tuple[Tree[list[T]]]
2. Apply T := int:       int | tuple[Tree[list[int]]]
```

Step 1 replaces the recursive variable `F[list[T]]` with the application `Tree[list[T]]`. Step 2 can then use ordinary
specialization on a closed type, replacing `T` with `int`.

At the next layer down, unfolding works the same, except now `T` is `list[int]`:

```text
Stored body:             T | tuple[F[list[T]]]
1. Close the body:       T | tuple[Tree[list[T]]]
2. Apply T := list[int]: list[int] | tuple[Tree[list[list[int]]]]
```

[Generic recursive types][recursive-source] · [Close, then specialize][unfold-source]

______________________________________________________________________

## 7. What this enables, and what remains to be done

Checking `((("leaf",),),)` from the opening example now follows each tuple element as another `Tree`.
At the leaf, `"leaf"` is neither an `int` nor a one-element tuple, so we reject the return.

The implementation carries these recursive types through subscripting, member lookup, calls,
assignability, narrowing, and other type operations.

This PR applies the new representation to recursive implicit and PEP 613 (`TypeAlias`) aliases.

PEP 695 aliases don't yet use this new approach; they still use `Type::TypeAlias` with implicit
recursive nesting. This mostly works fine but sometimes collapses to `Divergent`. A future PR should
also use the new representation for recursive PEP 695 aliases.

Recursive protocols should also be adapted to use `RecursiveType`; this isn't done yet.

So we haven't yet eliminated the `Divergent` type, but that is the eventual goal.

Our `CycleDetector` guards for recursive traversals are still needed; otherwise traversals would
just unfold forever.

______________________________________________________________________

## Appendix: code to open during questions

| Question                                                          | Entry point                                                       |
| ----------------------------------------------------------------- | ----------------------------------------------------------------- |
| Where are the representation and binding rules defined?           | [`types/recursive.rs`][recursive-source]                          |
| Where does alias inference get its initial recursive value?       | [`infer_implicit_alias_type`][inference-source]                   |
| How does recovery turn inferred occurrences into bound variables? | [`RecursiveType::initial`, `recover`, and `bind`][binding-source] |
| Where is the order of generic unfolding implemented?              | [`RecursiveType::unfolded_body`][unfold-source]                   |
| How does an existing operation use the new representation?        | [`Type::subscript`][subscript-source]                             |
| Where are the behavioral examples?                                | [`implicit_type_aliases.md`][nested-test]                         |

### Mutual recursion

Mutual recursion does not necessarily require one binder per alias. These two aliases alternate the
accepted leaf type at each tuple layer:

```python
Even = int | tuple["Odd"]
Odd = str | tuple["Even"]

def valid_even() -> Even:
    return ((1,),)

def invalid_even() -> Even:
    return (("leaf",),)  # error: invalid-return-type
```

When inference starts with `Even`, its `infer_implicit_alias_type` query needs the type of `Odd`,
which in turn needs `Even`. That second request for `Even` encounters the query cycle. With this
inference order, we end up with one `RecursiveType` and one cycle identity: `Odd` refers to `Even`,
while the body of `Even` inlines the definition of `Odd`:

```text
Even = μe. (int | tuple[str | tuple[e]])
Odd  = str | tuple[Even]
```

Starting inference with `Odd` reverses these roles.

[Mutual recursion test][mutual-test]

[binding-source]: https://github.com/astral-sh/ruff/blob/160ffbf2cff30d7ff1c97f13d6f1ca9fe430fe48/crates/ty_python_semantic/src/types/recursive.rs#L202-L273
[inference-source]: https://github.com/astral-sh/ruff/blob/160ffbf2cff30d7ff1c97f13d6f1ca9fe430fe48/crates/ty_python_semantic/src/types/infer.rs#L83-L126
[map-or-else-source]: https://github.com/astral-sh/ruff/blob/160ffbf2cff30d7ff1c97f13d6f1ca9fe430fe48/crates/ty_python_semantic/src/types/recursive.rs#L508-L547
[mutual-test]: https://github.com/astral-sh/ruff/blob/160ffbf2cff30d7ff1c97f13d6f1ca9fe430fe48/crates/ty_python_semantic/resources/mdtest/implicit_type_aliases.md#L2329-L2347
[nested-test]: https://github.com/astral-sh/ruff/blob/160ffbf2cff30d7ff1c97f13d6f1ca9fe430fe48/crates/ty_python_semantic/resources/mdtest/implicit_type_aliases.md#L2093-L2119
[recursive-source]: https://github.com/astral-sh/ruff/blob/160ffbf2cff30d7ff1c97f13d6f1ca9fe430fe48/crates/ty_python_semantic/src/types/recursive.rs#L1-L172
[subscript-source]: https://github.com/astral-sh/ruff/blob/160ffbf2cff30d7ff1c97f13d6f1ca9fe430fe48/crates/ty_python_semantic/src/types/subscript.rs#L575-L588
[unfold-source]: https://github.com/astral-sh/ruff/blob/160ffbf2cff30d7ff1c97f13d6f1ca9fe430fe48/crates/ty_python_semantic/src/types/recursive.rs#L340-L383
