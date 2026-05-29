//! Newline-delimited JSON I/O for triples.
//!
//! The on-disk format is exactly what
//! `lance_graph::graph::spo::odoo_ontology::parse_triples` reads: one
//! `{"s","p","o","f","c"}` object per line. Writing through this module
//! guarantees the downstream store loads it without a transform.

use crate::triple::{Predicate, Triple};

/// Serialise triples to ndjson (one object per line, trailing newline).
///
/// Order is preserved as given — call [`crate::expand`] first if you want
/// the canonical sorted form.
#[must_use]
pub fn to_ndjson(triples: &[Triple]) -> String {
    let mut out = String::new();
    for t in triples {
        // serde_json on a flat 5-field struct cannot fail; fall back to a
        // skip rather than panicking if it somehow does.
        if let Ok(line) = serde_json::to_string(t) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

/// Parse ndjson into triples. Blank lines are skipped.
///
/// Returns `Err` with the 1-based line number of the first malformed line.
/// Unlike the downstream loader (which silently drops bad lines for
/// resilience), the extractor side fails loud so a corrupt emit is caught
/// at the source.
///
/// # Validation
///
/// Two failure modes per line:
///
/// 1. **JSON shape** — `serde_json::from_str` rejects lines that aren't a
///    well-formed `{s, p, o, f, c}` object.
/// 2. **Closed predicate vocabulary** — every `t.p` must round-trip through
///    [`crate::Predicate::from_str`]. A typo like `"depend_on"` (missing
///    `s`) parses as a JSON `Triple` but is rejected here, because such a
///    triple would silently disappear from downstream `depends_on` queries.
///    This matches the contract stated in [`crate::Predicate`]'s doc:
///    "frontends MUST NOT emit raw predicate strings".
pub fn from_ndjson(ndjson: &str) -> Result<Vec<Triple>, ParseError> {
    let mut out = Vec::new();
    for (i, line) in ndjson.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let t: Triple = match serde_json::from_str(line) {
            Ok(t) => t,
            Err(source) => {
                return Err(ParseError {
                    line: i + 1,
                    message: source.to_string(),
                });
            }
        };
        if Predicate::from_str(&t.p).is_none() {
            return Err(ParseError {
                line: i + 1,
                message: format!(
                    "unknown predicate `{}` (not in the closed vocabulary); \
                     valid predicates: rdf:type, has_function, emitted_by, \
                     depends_on, reads_field, raises, traverses_relation",
                    t.p
                ),
            });
        }
        out.push(t);
    }
    Ok(out)
}

/// A malformed ndjson line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line number of the offending line.
    pub line: usize,
    /// The underlying serde_json error message.
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "malformed ndjson at line {}: {}",
            self.line, self.message
        )
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triple::Provenance;

    fn sample() -> Vec<Triple> {
        vec![
            Triple::new(
                "odoo:account_move",
                Predicate::RdfType,
                "ogit:ObjectType",
                Provenance::Structural,
            ),
            Triple::new(
                "odoo:account_move.amount_total",
                Predicate::EmittedBy,
                "odoo:account_move._compute_amount",
                Provenance::Authoritative,
            ),
        ]
    }

    #[test]
    fn ndjson_round_trips() {
        let triples = sample();
        let text = to_ndjson(&triples);
        let parsed = from_ndjson(&text).expect("valid ndjson");
        assert_eq!(parsed, triples);
    }

    #[test]
    fn each_triple_is_one_line() {
        let text = to_ndjson(&sample());
        assert_eq!(text.lines().count(), 2);
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn blank_lines_skipped() {
        let text = "\n\n{\"s\":\"a\",\"p\":\"rdf:type\",\"o\":\"ogit:ObjectType\",\"f\":1.0,\"c\":1.0}\n\n";
        let parsed = from_ndjson(text).expect("valid");
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn malformed_line_reports_line_number() {
        let text = "{\"s\":\"a\",\"p\":\"rdf:type\",\"o\":\"ogit:ObjectType\",\"f\":1.0,\"c\":1.0}\nNOT JSON\n";
        let err = from_ndjson(text).expect_err("should fail");
        assert_eq!(err.line, 2);
    }

    /// Closed-vocabulary enforcement: a typo in the predicate field (e.g.
    /// `depend_on` instead of `depends_on`) parses as a well-formed JSON
    /// `Triple` but must be rejected — otherwise it disappears silently
    /// from downstream `depends_on` queries (codex P2 review on PR #4).
    #[test]
    fn unknown_predicate_is_rejected() {
        let text =
            "{\"s\":\"odoo:a.b\",\"p\":\"depend_on\",\"o\":\"odoo:a.c\",\"f\":0.95,\"c\":0.9}\n";
        let err = from_ndjson(text).expect_err("typo predicate must fail loud");
        assert_eq!(err.line, 1);
        assert!(
            err.message.contains("unknown predicate"),
            "error must name the closed-vocabulary violation, got: {}",
            err.message
        );
        assert!(
            err.message.contains("depend_on"),
            "error must echo the offending predicate, got: {}",
            err.message
        );
    }

    /// Every canonical predicate must parse cleanly — guards against
    /// `from_str` drifting away from the `to_ndjson` writer.
    #[test]
    fn every_canonical_predicate_parses() {
        for p in [
            Predicate::RdfType,
            Predicate::HasFunction,
            Predicate::EmittedBy,
            Predicate::DependsOn,
            Predicate::ReadsField,
            Predicate::Raises,
            Predicate::TraversesRelation,
        ] {
            let line = format!(
                "{{\"s\":\"x\",\"p\":\"{}\",\"o\":\"y\",\"f\":1.0,\"c\":1.0}}\n",
                p.as_str()
            );
            from_ndjson(&line)
                .unwrap_or_else(|e| panic!("canonical predicate {} rejected: {e}", p.as_str()));
        }
    }
}
