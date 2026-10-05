//! Opt-in observations of generated relations inside synchronous descriptor invocations.

use std::cell::RefCell;
use std::fmt::{Debug, Write as _};
use std::hash::{Hash, Hasher};
use std::io::Write as _;

use crate::types::Type;

const MAX_ROWS: usize = 100_000;

thread_local! {
    static TRACE: RefCell<Option<Trace>> = const { RefCell::new(None) };
}

struct Trace {
    output: std::fs::File,
    rows: usize,
    next_scope: usize,
    descriptors: Vec<usize>,
    pairs: Vec<usize>,
    truncated: bool,
    io_failed: bool,
}

impl Trace {
    fn emit(&mut self, label: &str, detail: impl Debug) {
        if self.io_failed || self.truncated {
            return;
        }
        if self.rows == MAX_ROWS {
            self.truncated = true;
            self.io_failed = writeln!(self.output, "TRUNCATED\tlimit={MAX_ROWS}").is_err();
            return;
        }
        self.io_failed = writeln!(
            self.output,
            "{}\t{label}\tdescriptor={:?}\tpair={:?}\tdepth={}\t{detail:?}",
            self.rows,
            self.descriptors.last(),
            self.pairs.last(),
            self.pairs.len(),
        )
        .is_err();
        self.rows += 1;
    }
}

pub(in crate::types) struct Installed;

pub(in crate::types) fn install(path: &str) -> anyhow::Result<Installed> {
    TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        anyhow::ensure!(trace.is_none(), "descriptor trace already installed");
        let mut output = std::fs::File::create(path)?;
        writeln!(
            output,
            "TRACE_START\tlimit={MAX_ROWS}\tidentity=hash-write-stream"
        )?;
        *trace = Some(Trace {
            output,
            rows: 0,
            next_scope: 0,
            descriptors: Vec::new(),
            pairs: Vec::new(),
            truncated: false,
            io_failed: false,
        });
        Ok(Installed)
    })
}

impl Drop for Installed {
    fn drop(&mut self) {
        TRACE.with(|trace| {
            if let Some(mut trace) = trace.borrow_mut().take() {
                let _ = writeln!(
                    trace.output,
                    "TRACE_END\trows={}\tdescriptors={}\tpairs={}\ttruncated={}\tio_failed={}",
                    trace.rows,
                    trace.descriptors.len(),
                    trace.pairs.len(),
                    trace.truncated,
                    trace.io_failed,
                );
            }
        });
    }
}

/// Details must contain scalar observations or immutable keys, never database-aware type display.
pub(in crate::types) fn event(label: &str, detail: impl Debug) {
    TRACE.with(|trace| {
        if let Some(trace) = trace.borrow_mut().as_mut() {
            trace.emit(label, detail);
        }
    });
}

pub(in crate::types) fn within_descriptor() -> bool {
    TRACE.with(|trace| {
        trace
            .borrow()
            .as_ref()
            .is_some_and(|trace| !trace.descriptors.is_empty())
    })
}

pub(in crate::types) fn enabled() -> bool {
    TRACE.with(|trace| trace.borrow().is_some())
}

/// Preserve each write and its boundary instead of reducing the immutable handle to a hash.
/// This representation is local to this binary and database; it performs no database reads.
pub(in crate::types) fn key(value: impl Hash) -> String {
    #[derive(Default)]
    struct Writes(String);

    impl Hasher for Writes {
        fn finish(&self) -> u64 {
            0
        }

        fn write(&mut self, bytes: &[u8]) {
            let _ = write!(self.0, "{}:", bytes.len());
            for byte in bytes {
                let _ = write!(self.0, "{byte:02x}");
            }
            self.0.push(';');
        }
    }

    let mut writes = Writes::default();
    value.hash(&mut writes);
    writes.0
}

#[derive(Clone, Copy)]
enum Kind {
    Descriptor,
    Pair,
}

pub(in crate::types) struct Scope {
    active: Option<(Kind, usize)>,
}

fn enter(kind: Kind, label: &str, detail: impl Debug) -> Scope {
    let marker = 0u8;
    let active = TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        let trace = trace.as_mut()?;
        let scope = trace.next_scope;
        trace.next_scope += 1;
        trace.emit(label, (scope, (&raw const marker).addr(), detail));
        match kind {
            Kind::Descriptor => trace.descriptors.push(scope),
            Kind::Pair => trace.pairs.push(scope),
        }
        Some((kind, scope))
    });
    Scope { active }
}

pub(in crate::types) fn descriptor(callable: Type<'_>, arguments: [Type<'_>; 3]) -> Scope {
    if !enabled() {
        return Scope { active: None };
    }
    enter(
        Kind::Descriptor,
        "DescriptorEnter",
        (key(callable), arguments.map(key)),
    )
}

pub(in crate::types) fn pair(detail: impl Debug) -> Scope {
    enter(Kind::Pair, "PairEnter", detail)
}

impl Drop for Scope {
    fn drop(&mut self) {
        let Some((kind, scope)) = self.active else {
            return;
        };
        TRACE.with(|trace| {
            let mut trace = trace.borrow_mut();
            let Some(trace) = trace.as_mut() else { return };
            let label = match kind {
                Kind::Descriptor => "DescriptorExit",
                Kind::Pair => "PairExit",
            };
            trace.emit(label, scope);
            match kind {
                Kind::Descriptor => trace.descriptors.pop(),
                Kind::Pair => trace.pairs.pop(),
            };
        });
    }
}
