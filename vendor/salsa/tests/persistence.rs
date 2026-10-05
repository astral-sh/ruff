#![cfg(all(feature = "persistence", feature = "inventory"))]

mod common;

use common::LogDatabase;
use salsa::{Database, Durability, Setter};

use expect_test::expect;

#[salsa::input(persist)]
struct MyInput {
    #[returns(copy)]
    field: usize,
}

#[salsa::input(persist, singleton)]
struct MySingleton {
    #[returns(copy)]
    field: usize,
}

#[salsa::interned(persist)]
struct MyInterned<'db> {
    field: String,
}

#[salsa::tracked(persist)]
struct MyTracked<'db> {
    field: String,
}

#[salsa::tracked(returns(copy), persist)]
fn unit_to_interned(db: &dyn salsa::Database) -> MyInterned<'_> {
    MyInterned::new(db, "a".repeat(50))
}

#[salsa::tracked(returns(copy), persist)]
fn input_to_tracked(db: &dyn salsa::Database, input: MyInput) -> MyTracked<'_> {
    MyTracked::new(db, "a".repeat(input.field(db)))
}

#[salsa::tracked(returns(clone), persist)]
fn input_pair_to_string(db: &dyn salsa::Database, input1: MyInput, input2: MyInput) -> String {
    "a".repeat(input1.field(db) + input2.field(db))
}

#[test]
fn everything() {
    let mut db = common::LoggerDatabase::default();

    let _input1 = MyInput::new(&db, 1);
    let _input2 = MyInput::new(&db, 2);

    let serialized =
        serde_json::to_string_pretty(&<dyn salsa::Database>::as_serialize(&mut db)).unwrap();

    let expected = expect![[r#"
        {
          "runtime": {
            "revisions": [
              1,
              1,
              1
            ]
          },
          "ingredients": {
            "0": {
              "1": {
                "durabilities": [
                  0
                ],
                "revisions": [
                  1
                ],
                "fields": [
                  1
                ]
              },
              "2": {
                "durabilities": [
                  0
                ],
                "revisions": [
                  1
                ],
                "fields": [
                  2
                ]
              }
            }
          }
        }"#]];

    expected.assert_eq(&serialized);

    let input1 = MyInput::new(&db, 1);
    let input2 = MyInput::new(&db, 2);
    let _singleton = MySingleton::new(&db, 1);

    let _out = unit_to_interned(&db);
    let _out = input_to_tracked(&db, input1);
    let _out = input_pair_to_string(&db, input1, input2);

    let serialized =
        serde_json::to_string_pretty(&<dyn salsa::Database>::as_serialize(&mut db)).unwrap();

    let expected = expect![[r#"
        {
          "runtime": {
            "revisions": [
              1,
              1,
              1
            ]
          },
          "ingredients": {
            "0": {
              "1": {
                "durabilities": [
                  0
                ],
                "revisions": [
                  1
                ],
                "fields": [
                  1
                ]
              },
              "2": {
                "durabilities": [
                  0
                ],
                "revisions": [
                  1
                ],
                "fields": [
                  2
                ]
              },
              "3": {
                "durabilities": [
                  0
                ],
                "revisions": [
                  1
                ],
                "fields": [
                  1
                ]
              },
              "4": {
                "durabilities": [
                  0
                ],
                "revisions": [
                  1
                ],
                "fields": [
                  2
                ]
              }
            },
            "2": {
              "129": {
                "durabilities": [
                  0
                ],
                "revisions": [
                  1
                ],
                "fields": [
                  1
                ]
              }
            },
            "4": {
              "385": {
                "durability": 3,
                "last_interned_at": 1,
                "fields": [
                  "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                ]
              }
            },
            "5": {
              "513": {
                "durability": 0,
                "updated_at": 1,
                "revisions": [],
                "fields": [
                  "a"
                ]
              }
            },
            "7": {
              "641": {
                "durability": 3,
                "last_interned_at": 18446744073709551615,
                "fields": [
                  3,
                  4
                ]
              }
            },
            "19": {
              "257": {
                "durability": 3,
                "last_interned_at": 18446744073709551615,
                "fields": null
              }
            },
            "6": {
              "7:641": {
                "value": "aaa",
                "verified_at": 1,
                "revisions": {
                  "changed_at": 1,
                  "durability": 0,
                  "origin": {
                    "Derived": [
                      [
                        3,
                        1
                      ],
                      [
                        4,
                        1
                      ]
                    ]
                  },
                  "verified_final": true,
                  "extra": null
                }
              }
            },
            "8": {
              "0:3": {
                "value": 513,
                "verified_at": 1,
                "revisions": {
                  "changed_at": 1,
                  "durability": 0,
                  "origin": {
                    "Derived": [
                      [
                        3,
                        1
                      ]
                    ]
                  },
                  "verified_final": true,
                  "extra": {
                    "output_order": "Execution",
                    "tracked_struct_ids": [
                      [
                        {
                          "ingredient_index": 5,
                          "hash": 6073466998405137972,
                          "disambiguator": 0
                        },
                        513
                      ]
                    ],
                    "cycle_heads": []
                  }
                }
              }
            },
            "18": {
              "19:257": {
                "value": 385,
                "verified_at": 1,
                "revisions": {
                  "changed_at": 1,
                  "durability": 3,
                  "origin": {
                    "Derived": []
                  },
                  "verified_final": true,
                  "extra": null
                }
              }
            }
          }
        }"#]];

    expected.assert_eq(&serialized);

    let mut db = common::EventLoggerDatabase::default();
    <dyn salsa::Database>::deserialize(
        &mut db,
        &mut serde_json::Deserializer::from_str(&serialized),
    )
    .unwrap();

    assert_eq!(MySingleton::get(&db).field(&db), 1);

    let _out = unit_to_interned(&db);
    let _out = input_to_tracked(&db, input1);
    let _out = input_pair_to_string(&db, input1, input2);

    // The structs are not recreated, and the queries are not re-executed.
    db.assert_logs(expect![[r#"
        [
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillCheckCancellation",
            "WillCheckCancellation",
        ]"#]]);
}

#[test]
fn partial_query() {
    use salsa::plumbing::ZalsaDatabase;

    #[salsa::tracked(returns(copy), persist)]
    fn query(db: &dyn salsa::Database, input: MyInput) -> usize {
        inner_query(db, input) + 1
    }

    // Note that the inner query is not persisted, but we should still preserve the dependency on `input.field`.
    #[salsa::tracked(returns(copy))]
    fn inner_query(db: &dyn salsa::Database, input: MyInput) -> usize {
        input.field(db)
    }

    let mut db = common::EventLoggerDatabase::default();

    let input = MyInput::new(&db, 0);

    let result = query(&db, input);
    assert_eq!(result, 1);

    let serialized =
        serde_json::to_string_pretty(&<dyn salsa::Database>::as_serialize(&mut db)).unwrap();
    let expected = expect![[r#"
        {
          "runtime": {
            "revisions": [
              1,
              1,
              1
            ]
          },
          "ingredients": {
            "0": {
              "1": {
                "durabilities": [
                  0
                ],
                "revisions": [
                  1
                ],
                "fields": [
                  0
                ]
              }
            },
            "13": {
              "0:1": {
                "value": 1,
                "verified_at": 1,
                "revisions": {
                  "changed_at": 1,
                  "durability": 0,
                  "origin": {
                    "Derived": [
                      [
                        1,
                        1
                      ]
                    ]
                  },
                  "verified_final": true,
                  "extra": null
                }
              }
            }
          }
        }"#]];
    expected.assert_eq(&serialized);

    let mut db = common::EventLoggerDatabase::default();
    <dyn salsa::Database>::deserialize(
        &mut db,
        &mut serde_json::Deserializer::from_str(&serialized),
    )
    .unwrap();

    let input = MyInput::ingredient(&db)
        .entries(db.zalsa())
        .next()
        .unwrap()
        .as_struct();

    let result = query(&db, input);
    assert_eq!(result, 1);

    // The query was not re-executed.
    db.assert_logs(expect![[r#"
        [
            "DidSetCancellationFlag",
            "WillCheckCancellation",
        ]"#]]);

    input.set_field(&mut db).to(1);

    let result = query(&db, input);
    assert_eq!(result, 2);

    // The query was re-executed afer the input was updated.
    db.assert_logs(expect![[r#"
        [
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: query(Id(0)) }",
            "WillCheckCancellation",
            "WillExecute { database_key: inner_query(Id(0)) }",
        ]"#]]);
}

#[test]
fn partial_query_interned() {
    use salsa::plumbing::{AsId, ZalsaDatabase};

    #[salsa::tracked(returns(copy), persist)]
    fn intern(db: &dyn salsa::Database, input: MyInput, value: usize) -> MyInterned<'_> {
        do_intern(db, input, value)
    }

    // Note that the inner query is not persisted, but we should still preserve the dependency on `MyInterned`.
    #[salsa::tracked(returns(copy))]
    fn do_intern(db: &dyn salsa::Database, input: MyInput, value: usize) -> MyInterned<'_> {
        let _i = input.field(db); // Only low durability interned values are garbage collected.
        MyInterned::new(db, value.to_string())
    }

    let mut db = common::EventLoggerDatabase::default();
    let input = MyInput::builder(0).durability(Durability::LOW).new(&db);

    // Intern `i0`.
    let i0 = intern(&db, input, 0);
    assert_eq!(i0.field(&db), "0");

    let serialized =
        serde_json::to_string_pretty(&<dyn salsa::Database>::as_serialize(&mut db)).unwrap();
    let expected = expect![[r#"
        {
          "runtime": {
            "revisions": [
              1,
              1,
              1
            ]
          },
          "ingredients": {
            "0": {
              "1": {
                "durabilities": [
                  0
                ],
                "revisions": [
                  1
                ],
                "fields": [
                  0
                ]
              }
            },
            "4": {
              "385": {
                "durability": 0,
                "last_interned_at": 1,
                "fields": [
                  "0"
                ]
              }
            },
            "17": {
              "129": {
                "durability": 3,
                "last_interned_at": 18446744073709551615,
                "fields": [
                  1,
                  0
                ]
              }
            },
            "16": {
              "17:129": {
                "value": 385,
                "verified_at": 1,
                "revisions": {
                  "changed_at": 1,
                  "durability": 0,
                  "origin": {
                    "Derived": [
                      [
                        1,
                        1
                      ],
                      [
                        385,
                        4
                      ]
                    ]
                  },
                  "verified_final": true,
                  "extra": null
                }
              }
            }
          }
        }"#]];
    expected.assert_eq(&serialized);

    let mut db = common::EventLoggerDatabase::default();
    <dyn salsa::Database>::deserialize(
        &mut db,
        &mut serde_json::Deserializer::from_str(&serialized),
    )
    .unwrap();

    let input = MyInput::ingredient(&db)
        .entries(db.zalsa())
        .next()
        .unwrap()
        .as_struct();

    // Directly re-intern the restored value to exercise the rebuilt key map.
    let restored_i0_id = MyInterned::ingredient(db.zalsa())
        .entries(db.zalsa())
        .next()
        .unwrap()
        .key()
        .key_index();
    assert_eq!(
        MyInterned::new(&db, "0".to_string()).as_id(),
        restored_i0_id
    );

    // Re-intern `i0`.
    let i0 = intern(&db, input, 0);
    let i0_id = i0.as_id();
    assert_eq!(i0.field(&db), "0");

    // The query was not re-executed.
    db.assert_logs(expect![[r#"
        [
            "DidSetCancellationFlag",
            "WillCheckCancellation",
        ]"#]]);

    // Get the garbage collector to consider `i0` stale.
    for x in 1.. {
        db.synthetic_write(Durability::LOW);

        let ix = intern(&db, input, x);
        let ix_id = ix.as_id();

        // We reused the slot of `i0`.
        if ix_id.index() == i0_id.index() {
            break;
        }
    }

    // Re-intern `i0` after is has been garbage collected.
    let i0 = intern(&db, input, 0);

    // The query was re-executed due to garbage collection, even though no inputs have changed
    // and the inner query was not persisted.
    assert_eq!(i0.field(&db), "0");
    assert_ne!(i0_id.index(), i0.as_id().index());
}

#[test]
#[should_panic(expected = "must be persistable")]
fn invalid_specified_dependency() {
    #[salsa::tracked(returns(copy))]
    fn specify(db: &dyn salsa::Database) {
        let tracked = MyTracked::new(db, "a".to_string());
        specified_query::specify(db, tracked, 2222);
    }

    #[salsa::tracked(returns(copy), specify, persist)]
    fn specified_query<'db>(_db: &'db dyn salsa::Database, _tracked: MyTracked<'db>) -> u32 {
        0
    }

    let mut db = common::LoggerDatabase::default();

    specify(&db);

    let _serialized =
        serde_json::to_string_pretty(&<dyn salsa::Database>::as_serialize(&mut db)).unwrap();
}

#[test]
fn serialize_nothing() {
    let mut db = common::LoggerDatabase::default();

    let serialized =
        serde_json::to_string_pretty(&<dyn salsa::Database>::as_serialize(&mut db)).unwrap();

    // Empty ingredients should not be serialized.
    let expected = expect![[r#"
        {
          "runtime": {
            "revisions": [
              1,
              1,
              1
            ]
          },
          "ingredients": {}
        }"#]];

    expected.assert_eq(&serialized);
}
