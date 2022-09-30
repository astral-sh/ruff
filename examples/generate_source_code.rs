use anyhow::Result;
use rustpython_ast::{Alias, AliasData, Stmt, StmtKind};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Location};

use ruff::unparse::{RuffExpr, RuffStmt};

fn main() -> Result<()> {
    let expr = RuffExpr::new(Expr::new(
        Location::default(),
        ExprKind::Constant {
            value: Constant::Bool(true),
            kind: None,
        },
    ));
    println!("{}", expr);

    let stmt = RuffStmt::new(Stmt::new(Location::default(), StmtKind::Pass));
    println!("{}", stmt);

    let stmt = RuffStmt::new(Stmt::new(
        Location::default(),
        StmtKind::Return {
            value: Some(Box::new(Expr::new(
                Location::default(),
                ExprKind::Set { elts: vec![] },
            ))),
        },
    ));
    println!("{}", stmt);

    let stmt = RuffStmt::new(Stmt::new(
        Location::default(),
        StmtKind::Delete {
            targets: vec![Expr::new(
                Location::default(),
                ExprKind::Set { elts: vec![] },
            )],
        },
    ));
    println!("{}", stmt);

    let stmt = RuffStmt::new(Stmt::new(
        Location::default(),
        StmtKind::Import {
            names: vec![
                Alias::new(
                    Location::default(),
                    AliasData {
                        name: "foo".to_string(),
                        asname: Some("bar".to_string()),
                    },
                ),
                Alias::new(
                    Location::default(),
                    AliasData {
                        name: "baz".to_string(),
                        asname: Some("bar".to_string()),
                    },
                ),
            ],
        },
    ));
    println!("{}", stmt);

    let stmt = RuffStmt::new(Stmt::new(
        Location::default(),
        StmtKind::If {
            test: Box::new(Expr::new(
                Location::default(),
                ExprKind::Constant {
                    value: Constant::Bool(true),
                    kind: None,
                },
            )),
            body: vec![Stmt::new(
                Location::default(),
                StmtKind::Import {
                    names: vec![
                        Alias::new(
                            Location::default(),
                            AliasData {
                                name: "foo".to_string(),
                                asname: Some("bar".to_string()),
                            },
                        ),
                        Alias::new(
                            Location::default(),
                            AliasData {
                                name: "baz".to_string(),
                                asname: Some("bar".to_string()),
                            },
                        ),
                    ],
                },
            )],
            orelse: vec![],
        },
    ));
    println!("{}", stmt);

    let stmt = RuffStmt::new(Stmt::new(
        Location::default(),
        StmtKind::Assert {
            test: Box::new(Expr::new(
                Location::default(),
                ExprKind::Constant {
                    value: Constant::Bool(true),
                    kind: None,
                },
            )),
            msg: Some(Box::new(Expr::new(
                Location::default(),
                ExprKind::Constant {
                    value: Constant::Str("Bad".to_string()),
                    kind: None,
                },
            ))),
        },
    ));
    println!("{}", stmt);

    Ok(())
}
