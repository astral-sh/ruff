#![cfg(all(feature = "inventory", feature = "accumulator"))]

//! Some historical tests related to accumulated values and fixpoint iteration.
//!
//! All these tests are currently expected to panic because accumulated values
//! require preserving the query's dependency tree in full, but fixpoint iteration
//! now flattens the cycle head dependencies, breaking accumulated values.
//!
//! We keep this tests around as they're a good starting point if we decide
//! to support accumulated values in fixpoint queries.
//!
//! One test that should be added which is a case that was broken even before
//! we migrated fixpoint iteration to flatten the cycle head dependencies is
//! a case roughly like this:
//!
//! * `a` (outer most cycle head, calls `c` in every iteration)
//! * `b` (inner cycle), calls `c` only in the first iteration
//! * `c` calls `a` and `b` and creates an accumulated value in each iteration.
//!
//! The query `b` finalizes after the first iteration. The accumulated values of
//! `b` should only include the accumulated values of `c` from the **first** iteration.
//!
//! Using today's dependency traversal, this would require expressing the iteration count
//! in the dependency tree, so that the accumulator knows from which iteration
//! to aggregate the accumulated values from.

use std::collections::HashSet;

mod common;
use common::{LogDatabase, LoggerDatabase};
use expect_test::expect;
use salsa::{Accumulator, Setter};
use test_log::test;

#[salsa::input(debug)]
struct File {
    name: String,
    dependencies: Vec<File>,
    issues: Vec<u32>,
}

#[salsa::accumulator]
#[derive(Debug)]
struct Diagnostic(#[allow(dead_code)] String);

#[salsa::tracked(returns(clone), cycle_fn = cycle_fn, cycle_initial = cycle_initial)]
fn check_file(db: &dyn LogDatabase, file: File) -> Vec<u32> {
    db.push_log(format!(
        "check_file(name = {}, issues = {:?})",
        file.name(db),
        file.issues(db)
    ));

    let mut collected_issues = HashSet::<u32>::from_iter(file.issues(db).iter().copied());

    for &dep in file.dependencies(db) {
        let issues = check_file(db, dep);
        collected_issues.extend(issues);
    }

    let mut sorted_issues = collected_issues.iter().copied().collect::<Vec<_>>();
    sorted_issues.sort();

    for issue in &sorted_issues {
        Diagnostic(format!("file {}: issue {}", file.name(db), issue)).accumulate(db);
    }

    sorted_issues
}

fn cycle_initial(_db: &dyn LogDatabase, _id: salsa::Id, _file: File) -> Vec<u32> {
    vec![]
}

fn cycle_fn(
    _db: &dyn LogDatabase,
    _cycle: &salsa::Cycle,
    _last_provisional_value: &[u32],
    value: Vec<u32>,
    _file: File,
) -> Vec<u32> {
    value
}

#[test]
fn accumulate_once() {
    let db = LoggerDatabase::default();

    let file = File::new(&db, "fn".to_string(), vec![], vec![1]);
    let diagnostics = check_file::accumulated::<Diagnostic>(&db, file);
    db.assert_logs(expect![[r#"
        [
            "check_file(name = fn, issues = [1])",
        ]"#]]);

    expect![[r#"
        [
            Diagnostic(
                "file fn: issue 1",
            ),
        ]"#]]
    .assert_eq(&format!("{diagnostics:#?}"));
}

#[test]
fn accumulate_with_dep() {
    let db = LoggerDatabase::default();

    let file_a = File::new(&db, "file_a".to_string(), vec![], vec![1]);
    let file_b = File::new(&db, "file_b".to_string(), vec![file_a], vec![2]);

    let diagnostics = check_file::accumulated::<Diagnostic>(&db, file_b);
    db.assert_logs(expect![[r#"
        [
            "check_file(name = file_b, issues = [2])",
            "check_file(name = file_a, issues = [1])",
        ]"#]]);

    expect![[r#"
        [
            Diagnostic(
                "file file_b: issue 1",
            ),
            Diagnostic(
                "file file_b: issue 2",
            ),
            Diagnostic(
                "file file_a: issue 1",
            ),
        ]"#]]
    .assert_eq(&format!("{diagnostics:#?}"));
}

#[test]
#[should_panic(expected = "doesn't support accumulated values")]
fn accumulate_with_cycle() {
    let mut db = LoggerDatabase::default();

    let file_a = File::new(&db, "file_a".to_string(), vec![], vec![1]);
    let file_b = File::new(&db, "file_b".to_string(), vec![file_a], vec![2]);
    file_a.set_dependencies(&mut db).to(vec![file_b]);

    let diagnostics = check_file::accumulated::<Diagnostic>(&db, file_b);
    db.assert_logs(expect![[r#"
        [
            "check_file(name = file_b, issues = [2])",
            "check_file(name = file_a, issues = [1])",
            "check_file(name = file_b, issues = [2])",
            "check_file(name = file_a, issues = [1])",
        ]"#]]);

    expect![[r#"
        [
            Diagnostic(
                "file file_b: issue 1",
            ),
            Diagnostic(
                "file file_b: issue 2",
            ),
            Diagnostic(
                "file file_a: issue 1",
            ),
            Diagnostic(
                "file file_a: issue 2",
            ),
        ]"#]]
    .assert_eq(&format!("{diagnostics:#?}"));
}

#[test]
#[should_panic(expected = "doesn't support accumulated values")]
fn accumulate_with_cycle_second_revision() {
    let mut db = LoggerDatabase::default();

    let file_a = File::new(&db, "file_a".to_string(), vec![], vec![1]);
    let file_b = File::new(&db, "file_b".to_string(), vec![file_a], vec![2]);
    file_a.set_dependencies(&mut db).to(vec![file_b]);

    let diagnostics = check_file::accumulated::<Diagnostic>(&db, file_b);
    db.assert_logs(expect![[r#"
        [
            "check_file(name = file_b, issues = [2])",
            "check_file(name = file_a, issues = [1])",
            "check_file(name = file_b, issues = [2])",
            "check_file(name = file_a, issues = [1])",
        ]"#]]);

    expect![[r#"
        [
            Diagnostic(
                "file file_b: issue 1",
            ),
            Diagnostic(
                "file file_b: issue 2",
            ),
            Diagnostic(
                "file file_a: issue 1",
            ),
            Diagnostic(
                "file file_a: issue 2",
            ),
        ]"#]]
    .assert_eq(&format!("{diagnostics:#?}"));

    file_b.set_issues(&mut db).to(vec![2, 3]);

    let diagnostics = check_file::accumulated::<Diagnostic>(&db, file_a);
    db.assert_logs(expect![[r#"
        [
            "check_file(name = file_a, issues = [1])",
            "check_file(name = file_b, issues = [2, 3])",
            "check_file(name = file_a, issues = [1])",
            "check_file(name = file_b, issues = [2, 3])",
        ]"#]]);

    expect![[r#"
        [
            Diagnostic(
                "file file_a: issue 1",
            ),
            Diagnostic(
                "file file_a: issue 2",
            ),
            Diagnostic(
                "file file_a: issue 3",
            ),
            Diagnostic(
                "file file_b: issue 1",
            ),
            Diagnostic(
                "file file_b: issue 2",
            ),
            Diagnostic(
                "file file_b: issue 3",
            ),
        ]"#]]
    .assert_eq(&format!("{diagnostics:#?}"));
}

#[test]
#[should_panic(expected = "doesn't support accumulated values")]
fn accumulate_add_cycle() {
    let mut db = LoggerDatabase::default();

    let file_a = File::new(&db, "file_a".to_string(), vec![], vec![1]);
    let file_b = File::new(&db, "file_b".to_string(), vec![file_a], vec![2]);

    let diagnostics = check_file::accumulated::<Diagnostic>(&db, file_b);
    db.assert_logs(expect![[r#"
        [
            "check_file(name = file_b, issues = [2])",
            "check_file(name = file_a, issues = [1])",
        ]"#]]);

    expect![[r#"
        [
            Diagnostic(
                "file file_b: issue 1",
            ),
            Diagnostic(
                "file file_b: issue 2",
            ),
            Diagnostic(
                "file file_a: issue 1",
            ),
        ]"#]]
    .assert_eq(&format!("{diagnostics:#?}"));

    file_a.set_dependencies(&mut db).to(vec![file_b]);

    let diagnostics = check_file::accumulated::<Diagnostic>(&db, file_a);
    db.assert_logs(expect![[r#"
        [
            "check_file(name = file_a, issues = [1])",
            "check_file(name = file_b, issues = [2])",
            "check_file(name = file_a, issues = [1])",
            "check_file(name = file_b, issues = [2])",
        ]"#]]);

    expect![[r#"
        [
            Diagnostic(
                "file file_a: issue 1",
            ),
            Diagnostic(
                "file file_a: issue 2",
            ),
            Diagnostic(
                "file file_b: issue 1",
            ),
            Diagnostic(
                "file file_b: issue 2",
            ),
        ]"#]]
    .assert_eq(&format!("{diagnostics:#?}"));
}

#[test]
#[should_panic(expected = "doesn't support accumulated values")]
fn accumulate_remove_cycle() {
    let mut db = LoggerDatabase::default();

    let file_a = File::new(&db, "file_a".to_string(), vec![], vec![1]);
    let file_b = File::new(&db, "file_b".to_string(), vec![file_a], vec![2]);
    file_a.set_dependencies(&mut db).to(vec![file_b]);

    let diagnostics = check_file::accumulated::<Diagnostic>(&db, file_b);
    db.assert_logs(expect![[r#"
        [
            "check_file(name = file_b, issues = [2])",
            "check_file(name = file_a, issues = [1])",
            "check_file(name = file_b, issues = [2])",
            "check_file(name = file_a, issues = [1])",
        ]"#]]);

    expect![[r#"
        [
            Diagnostic(
                "file file_b: issue 1",
            ),
            Diagnostic(
                "file file_b: issue 2",
            ),
            Diagnostic(
                "file file_a: issue 1",
            ),
            Diagnostic(
                "file file_a: issue 2",
            ),
        ]"#]]
    .assert_eq(&format!("{diagnostics:#?}"));

    file_a.set_dependencies(&mut db).to(vec![]);

    let diagnostics = check_file::accumulated::<Diagnostic>(&db, file_b);
    db.assert_logs(expect![[r#"
        [
            "check_file(name = file_b, issues = [2])",
            "check_file(name = file_a, issues = [1])",
        ]"#]]);

    expect![[r#"
        [
            Diagnostic(
                "file file_b: issue 1",
            ),
            Diagnostic(
                "file file_b: issue 2",
            ),
            Diagnostic(
                "file file_a: issue 1",
            ),
        ]"#]]
    .assert_eq(&format!("{diagnostics:#?}"));
}
