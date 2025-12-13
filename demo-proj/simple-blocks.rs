fn main() {
    Block {
        nested: false,
        imports: [
            Import(StmtImport {
                node_index: NodeIndex(None),
                range: 0..9,
                names: [Alias {
                    range: 7..9,
                    node_index: NodeIndex(None),
                    name: Identifier {
                        id: Name("os"),
                        range: 7..9,
                        node_index: NodeIndex(None),
                    },
                    asname: None,
                }],
            }),
            Import(StmtImport {
                node_index: NodeIndex(None),
                range: 33..43,
                names: [Alias {
                    range: 40..43,
                    node_index: NodeIndex(None),
                    name: Identifier {
                        id: Name("sys"),
                        range: 40..43,
                        node_index: NodeIndex(None),
                    },
                    asname: None,
                }],
            }),
        ],
        trailer: Some(FunctionDef),
    };
}
fn main() {
    Block {
        nested: false,
        imports: [
            Import(StmtImport {
                node_index: NodeIndex(None),
                range: 0..9,
                names: [Alias {
                    range: 7..9,
                    node_index: NodeIndex(None),
                    name: Identifier {
                        id: Name("os"),
                        range: 7..9,
                        node_index: NodeIndex(None),
                    },
                    asname: None,
                }],
            }),
            Import(StmtImport {
                node_index: NodeIndex(None),
                range: 10..20,
                names: [Alias {
                    range: 17..20,
                    node_index: NodeIndex(None),
                    name: Identifier {
                        id: Name("sys"),
                        range: 17..20,
                        node_index: NodeIndex(None),
                    },
                    asname: None,
                }],
            }),
        ],
        trailer: None,
    };
}
