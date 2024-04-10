#[cfg(test)]
mod tests {
    use crate::parse_suite;

    #[test]
    fn test_with_statement() {
        let source = "\
with 0: pass
with 0 as x: pass
with 0, 1: pass
with 0 as x, 1 as y: pass
with 0 if 1 else 2: pass
with 0 if 1 else 2 as x: pass
with (): pass
with () as x: pass
with (0): pass
with (0) as x: pass
with (0,): pass
with (0,) as x: pass
with (0, 1): pass
with (0, 1) as x: pass
with (*a,): pass
with (*a,) as x: pass
with (0, *a): pass
with (0, *a) as x: pass
with (a := 0): pass
with (a := 0) as x: pass
with (a := 0, b := 1): pass
with (a := 0, b := 1) as x: pass
with (0 as a): pass
with (0 as a,): pass
with (0 as a, 1 as b): pass
with (0 as a, 1 as b,): pass
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_parenthesized_with_statement() {
        let source = "\
with ((a), (b)): pass
with ((a), (b), c as d, (e)): pass
with (a, b): pass
with (a, b) as c: pass
with ((a, b) as c): pass
with (a as b): pass
with (a): pass
with (a := 0): pass
with (a := 0) as x: pass
with ((a)): pass
with ((a := 0)): pass
with (a as b, (a := 0)): pass
with (a, (a := 0)): pass
with (yield): pass
with (yield from a): pass
with ((yield)): pass
with ((yield from a)): pass
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_with_statement_invalid() {
        for source in [
            "with 0,: pass",
            "with 0 as x,: pass",
            "with *a: pass",
            "with *a as x: pass",
            "with (*a): pass",
            "with (*a) as x: pass",
            "with *a, 0 as x: pass",
            "with (*a, 0 as x): pass",
            "with 0 as x, *a: pass",
            "with (0 as x, *a): pass",
            "with (0 as x) as y: pass",
            "with (0 as x), 1: pass",
            "with ((0 as x)): pass",
            "with a := 0 as x: pass",
            "with (a := 0 as x): pass",
        ] {
            assert!(parse_suite(source).is_err());
        }
    }

    #[test]
    fn test_invalid_type() {
        // TODO(dhruvmanila): Check error recovery for these cases
        assert!(parse_suite("a: type X = int").is_err());
        assert!(parse_suite("lambda: type X = int").is_err());
    }
}
