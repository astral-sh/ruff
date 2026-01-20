This is a markdown document with some fenced code blocks with unformatted code.

Unlabeled Python code:

```
print( "hello" )
```

Unlabeled Rust code:

```
fn thing() {
    println!( "hello" );
}
```

Labeled Python code:

```py
print( "hello" )
def foo(): pass
```

Labeled Python stub:

```pyi
print( "hello" )
def foo(): pass
```

Labeled Rust code:
```rust
fn thing() {
    println!( "hello" );
}
```

Indented code blocks should also work:

* List item

  ```py
  print( "hello" )
  ```

Block quoted code blocks may not be supported:

> Quoted text
>
> ```py
> print( "hello" )
> ```


Blacken-docs supports ignore directives:

<!-- blacken-docs:off -->
```py
print( "hello" )
```
<!-- blacken-docs:on -->

```py
print( "hello" )
```
