use tree_sitter::{Node, Tree, TreeCursor};

use super::*;

// TODO replace kind strings with ids resolved by a macro built at compile time

impl<'a> Located for Node<'a> {
    #[inline]
    fn start_row(&self) -> usize {
        self.start().0
    }

    #[inline]
    fn start_col(&self) -> usize {
        self.start().1
    }

    #[inline]
    fn end_row(&self) -> usize {
        self.end().0
    }

    #[inline]
    fn end_col(&self) -> usize {
        self.end().1
    }

    #[inline]
    fn start(&self) -> (usize, usize) {
        let rv = self.start_position();
        (rv.row, rv.column)
    }

    #[inline]
    fn end(&self) -> (usize, usize) {
        let rv = self.end_position();
        (rv.row, rv.column)
    }
}

impl<'a> Ident for Node<'a> {
    fn val(&self) -> &str {
        // NOTE nodes that count as identifiers:
        // - "identifier"
        // - "dotted_name"
        unimplemented!();
    }
}

impl BigInt for &str {}

// below implemented for dotted_name and aliased_import
impl<'a> Alias for Node<'a> {
    type Ident = Node<'a>;

    fn name(&self) -> &Self::Ident {
        match self.kind() {
            "dotted_name" => self,
            "aliased_import" => &self.child_by_field_name("dotted_name").unwrap(),
            _ => unreachable!(),
        }
    }

    fn asname(&self) -> Option<&Self::Ident> {
        match self.kind() {
            "dotted_name" => None,
            "aliased_import" => Some(&self.child_by_field_name("identifier").unwrap()),
            _ => unreachable!(),
        }
    }
}

impl<'a> Arg<'a> for Node<'a> {
    type Expr = Node<'a>;
    type Ident = Node<'a>;
}

struct PosonlyargsIter<'a> {
    cursor: Option<TreeCursor<'a>>,
}

impl<'a> PosonlyargsIter<'a> {
    fn new(parameters: &Node<'a>) -> Self {
        let cursor = if parameters
            .child_by_field_name("list_splat_pattern")
            .is_some()
        {
            let mut cursor = parameters.walk();
            cursor.goto_first_child();
            Some(cursor)
        } else {
            None
        };
        Self { cursor }
    }
}

impl<'a> Iterator for PosonlyargsIter<'a> {
    type Item = Node<'a>;

    fn next(&mut self) -> Option<Node<'a>> {
        if let Some(cursor) = &mut self.cursor {
            let mut rv = cursor.node();
            // creation of this asserts that list_splat_pattern exists in the cursors
            // siblings
            while rv.kind() != "list_splat_pattern" {
                if rv.kind() == "identifier" {
                    return Some(rv);
                }
                cursor.goto_next_sibling();
                return Some(rv);
            }
        }
        self.cursor = None;
        None
    }
}

// below implemented for "parameters"
impl<'a> Arguments<'a> for Node<'a> {
    type Arg = Node<'a>;
    type Expr = Node<'a>;
    type PosonlyargsIter<'b> = PosonlyargsIter<'b>;

    fn posonlyargs(&self) -> Self::PosonlyargsIter<'_> {
        PosonlyargsIter::new(self)
    }
}

impl<'a> FunctionDef<'a> for Node<'a> {
    type Arguments = Node<'a>;
    type BodyIter<'b> = Box<dyn Iterator<Item = Node<'a>>>;
    type Expr = Node<'a>;
    type Ident = Node<'a>;
    type Stmt = Node<'a>;

    fn name(&self) -> &Self::Ident {
        let function_definition = match self.kind() {
            "decorated_definition" => &self.child_by_field_name("function_definition").unwrap(),
            "function_definition" => self,
            _ => unreachable!(),
        };
        &self.child_by_field_name("identifier").unwrap()
    }

    fn args(&self) -> &Self::Arguments {
        let function_definition = match self.kind() {
            "decorated_definition" => &self.child_by_field_name("function_definition").unwrap(),
            "function_definition" => self,
            _ => unreachable!(),
        };
        &self.child_by_field_name("parameters").unwrap()
    }
}

impl<'a> Ast<'a> for Tree {
    type Alias = Node<'a>;
    type AnnAssign = Node<'a>;
    type Arg = Node<'a>;
    type Arguments = Node<'a>;
    type Assert = Node<'a>;
    type Assign = Node<'a>;
    type AsyncFor = Node<'a>;
    type AsyncFunctionDef = Node<'a>;
    type AsyncWith = Node<'a>;
    type Attribute = Node<'a>;
    type AugAssign = Node<'a>;
    type Await = Node<'a>;
    type BigInt = &str;
    type BinOp = Node<'a>;
    type BoolOp = Node<'a>;
    type Call = Node<'a>;
    type ClassDef = Node<'a>;
    type Compare = Node<'a>;
    type Comprehension = Node<'a>;
    type Constant = Node<'a>;
    type ConstantExpr = Node<'a>;
    type Delete = Node<'a>;
    type Dict = Node<'a>;
    type DictComp = Node<'a>;
    type ExceptHandler = Node<'a>;
    type Expr = Node<'a>;
    type For = Node<'a>;
    type FormattedValue = Node<'a>;
    type FunctionDef = Node<'a>;
    type GeneratorExp = Node<'a>;
    type Global = Node<'a>;
    type Ident = Node<'a>;
    type If = Node<'a>;
    type IfExp = Node<'a>;
    type Import = Node<'a>;
    type ImportFrom = Node<'a>;
    type JoinedStr = Node<'a>;
    type Keyword = Node<'a>;
    type Lambda = Node<'a>;
    type List = Node<'a>;
    type ListComp = Node<'a>;
    type Match = Node<'a>;
    type MatchAs = Node<'a>;
    type MatchCase = Node<'a>;
    type MatchClass = Node<'a>;
    type MatchMapping = Node<'a>;
    type MatchOr = Node<'a>;
    type MatchSequence = Node<'a>;
    type MatchSingleton = Node<'a>;
    type MatchStar = Node<'a>;
    type MatchValue = Node<'a>;
    type Name = Node<'a>;
    type NamedExpr = Node<'a>;
    type Nonlocal = Node<'a>;
    type Pattern = Node<'a>;
    type Raise = Node<'a>;
    type Return = Node<'a>;
    type Set = Node<'a>;
    type SetComp = Node<'a>;
    type Slice = Node<'a>;
    type Starred = Node<'a>;
    type Stmt = Node<'a>;
    // type StmtsIter<'b>: Iterator<Item = &'b Self::Stmt>
    type Subscript = Node<'a>;
    type Try = Node<'a>;
    type Tuple = Node<'a>;
    type UnaryOp = Node<'a>;
    type While = Node<'a>;
    type With = Node<'a>;
    type Withitem = Node<'a>;
    type Yield = Node<'a>;
    type YieldFrom = Node<'a>;

    fn stmts(&self) -> Self::StmtsIter<'_> {
        unimplemented!();
    }
}

#[cfg(test)]
mod tree_sitter_asserts {
    use super::*;

    // below creates every ast node for 3.10
    const ALL_AST_NODES_SNIPPET: &str = r#"
import sys
from sys import stdout as pain

X = 0

class F:
    def __matmul__(self, rhs):
        global X
        x: int
        X = 0
        x = ~X
        return x
    def __imatmul__(self, rhs):
        return self

async def g():
    with open():
        print("inside with", file=sys.stderr)
    async with open():
        list(*(0,))
    async for x in []:
        {x for x in x}
    if True:
        return
    await g()

def f(*_args):
    print("here", file=pain)
    v = f"{[-x+x-x*x/x//x%x&x^x|x@x<<x>>x for x in (x for x, _ in {x:x for x
in []})]}"     x = F()
    x @= x
    x = {"f":x} if x is x and not x is not x else None
    x = {x}
    while (x:= 0 not in x and x and x or (x and not x)):
        if True:
            break
        elif True:
            try:
                yield v
            except:
                yield from range(3)
        elif True:
            pass
        else:
            continue
    for x in []:
        +x[0]**2
    del x[0:0]

    def g():
        nonlocal v
        if v < 0 and v > 0 and v <= 0 and v >= 0 and v == 0 and v != 0 and 0
in v:             raise RuntimeError()

if __name__ == '__main__':
    match 0:
        case 0 | {1: _} | F() | None:
            (lambda: [])()
        case [x, *rest] if x > 0:
            assert 0 == 0
    f()

        "#;

    fn walk_tree<F: Fn(&Node) -> ()>(cursor: &mut tree_sitter::TreeCursor, op: &F) {
        loop {
            op(&cursor.node());
            if cursor.goto_first_child() {
                walk_tree(cursor, op);
            }
            if !cursor.goto_next_sibling() {
                cursor.goto_parent();
                break;
            }
        }
    }

    #[test]
    fn check_kind_ids() {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(tree_sitter_python::language()).unwrap();
        let tree = parser.parse(ALL_AST_NODES_SNIPPET, None).unwrap();
        let mut cursor = tree.walk();
        walk_tree(&mut cursor, &|node| {
            if let Some(kind) = NodeKind::from_id(node.kind_id()) {
                assert_eq!(kind.kind(), node.kind());
            }
        });
    }
}
