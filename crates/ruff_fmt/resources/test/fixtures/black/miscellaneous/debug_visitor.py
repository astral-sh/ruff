@dataclass
class DebugVisitor(Visitor[T]):
    tree_depth: int = 0

    def visit_default(self, node: LN) -> Iterator[T]:
        indent = ' ' * (2 * self.tree_depth)
        if isinstance(node, Node):
            _type = type_repr(node.type)
            out(f'{indent}{_type}', fg='yellow')
            self.tree_depth += 1
            for child in node.children:
                yield from self.visit(child)

            self.tree_depth -= 1
            out(f'{indent}/{_type}', fg='yellow', bold=False)
        else:
            _type = token.tok_name.get(node.type, str(node.type))
            out(f'{indent}{_type}', fg='blue', nl=False)
            if node.prefix:
                # We don't have to handle prefixes for `Node` objects since
                # that delegates to the first child anyway.
                out(f' {node.prefix!r}', fg='green', bold=False, nl=False)
            out(f' {node.value!r}', fg='blue', bold=False)

    @classmethod
    def show(cls, code: str) -> None:
        """Pretty-prints a given string of `code`.

        Convenience method for debugging.
        """
        v: DebugVisitor[None] = DebugVisitor()
        list(v.visit(lib2to3_parse(code)))
