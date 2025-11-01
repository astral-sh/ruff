Block {
  nested: false,
  imports: [
    Import(StmtImport {
      node_index: NodeIndex(None),
      range: 0..10,
      names: [Alias {
        range: 7..10,
        node_index: NodeIndex(None),
        name: Identifier {
          id: Name("bar"),
          range: 7..10,
          node_index: NodeIndex(None)
        },
        asname: None
      }]
    }),
    Import(StmtImport {
      node_index: NodeIndex(None),
      range: 11..21,
      names: [Alias {
        range: 18..21,
        node_index: NodeIndex(None),
        name: Identifier {
          id: Name("foo"),
          range: 18..21,
          node_index: NodeIndex(None)
        },
        asname: None
      }]
    }),
    Import(StmtImport {
      node_index: NodeIndex(None),
      range: 22..31,
      names: [Alias {
        range: 29..31,
        node_index: NodeIndex(None),
        name: Identifier {
          id: Name("os"),
          range: 29..31,
          node_index: NodeIndex(None)
        },
        asname: None
      }]
    })
  ],
  trailer: Some(FunctionDef)
}
Block {
  nested: false,
  imports: [
    Import(StmtImport {
      node_index: NodeIndex(None),
      range: 55..66,
      names: [Alias {
        range: 62..66,
        node_index: NodeIndex(None),
        name: Identifier {
          id: Name("last"),
          range: 62..66,
          node_index: NodeIndex(None)
        },
        asname: None
      }]
    }),
    Import(StmtImport {
      node_index: NodeIndex(None),
      range: 68..79,
      names: [Alias {
        range: 75..79,
        node_index: NodeIndex(None),
        name: Identifier {
          id: Name("late"),
          range: 75..79,
          node_index: NodeIndex(None)
        },
        asname: None
      }]
    }),
    ImportFrom(StmtImportFrom {
      node_index: NodeIndex(None),
      range: 92..140,
      module: Some(Identifier {
        id: Name("late_paren1"),
        range: 97..108,
        node_index: NodeIndex(None)
      }),
      names: [Alias {
        range: 133..138,
        node_index: NodeIndex(None),
        name: Identifier {
          id: Name("value"),
          range: 133..138,
          node_index: NodeIndex(None)
        },
        asname: None
      }],
      level: 0
    }),
    ImportFrom(StmtImportFrom {
      node_index: NodeIndex(None),
      range: 142..190,
      module: Some(Identifier {
        id: Name("late_paren2"),
        range: 147..158,
        node_index: NodeIndex(None)
      }),
      names: [Alias {
        range: 172..177,
        node_index: NodeIndex(None),
        name: Identifier {
          id: Name("value"),
          range: 172..177,
          node_index: NodeIndex(None)
        },
        asname: None
      }],
      level: 0
    }),
    ImportFrom(StmtImportFrom {
      node_index: NodeIndex(None),
      range: 192..229,
      module: Some(Identifier {
        id: Name("late_paren3"),
        range: 197..208,
        node_index: NodeIndex(None)
      }),
      names: [Alias {
        range: 222..227,
        node_index: NodeIndex(None),
        name: Identifier {
          id: Name("value"),
          range: 222..227,
          node_index: NodeIndex(None)
        },
        asname: None
      }],
      level: 0
    }),
    ImportFrom(StmtImportFrom {
      node_index: NodeIndex(None),
      range: 242..316,
      module: Some(Identifier {
        id: Name("late_paren4"),
        range: 247..258,
        node_index: NodeIndex(None)
      }),
      names: [
        Alias {
          range: 272..278,
          node_index: NodeIndex(None),
          name: Identifier {
            id: Name("value1"),
            range: 272..278,
            node_index: NodeIndex(None)
          },
          asname: None
        },
        Alias {
          range: 284..290,
          node_index: NodeIndex(None),
          name: Identifier {
            id: Name("value2"),
            range: 284..290,
            node_index: NodeIndex(None)
          },
          asname: None
        },
        Alias {
          range: 307..313,
          node_index: NodeIndex(None),
          name: Identifier {
            id: Name("value3"),
            range: 307..313,
            node_index: NodeIndex(None)
          },
          asname: None
        }
      ],
      level: 0
    })
  ],
  trailer: None
}
