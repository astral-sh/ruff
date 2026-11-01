use std::ops::Range;

use ruff_text_size::{Ranged, TextRange};

use crate::{NodeKind, Parameters};

use super::{ABSENT, BuildId, Builder, Interval, Row, offset, split_words};

#[derive(Clone, Copy)]
struct ParameterGroups {
    positional_only: u32,
    regular: u32,
    keyword_only: u32,
    vararg: u32,
    kwarg: u32,
}

impl Builder {
    /// Stores parameters read by the parser. Ordinary parameters are already in source
    /// order. The range identifies the positional-or-keyword group between the other two.
    pub fn parameters_from_slice(
        &mut self,
        range: TextRange,
        ordinary: &[BuildId<crate::kind::ParameterWithDefault>],
        regular: Range<usize>,
        vararg: Option<BuildId<crate::kind::Parameter>>,
        kwarg: Option<BuildId<crate::kind::Parameter>>,
    ) -> BuildId<crate::kind::Parameters> {
        if regular.start == 0
            && regular.end == ordinary.len()
            && vararg.is_none()
            && kwarg.is_none()
        {
            let items = self.list(ordinary.iter().copied()).word();
            return self.push_regular_parameters(range, items);
        }
        let start = self.ast.extra.len();
        let vararg = vararg.map_or(0, |node| node.slot);
        let kwarg = kwarg.map_or(0, |node| node.slot);
        self.ast
            .extra
            .extend(ordinary[..regular.end].iter().map(|node| node.slot));
        if vararg != 0 {
            self.ast.extra.push(vararg);
        }
        self.ast
            .extra
            .extend(ordinary[regular.end..].iter().map(|node| node.slot));
        if kwarg != 0 {
            self.ast.extra.push(kwarg);
        }
        let groups = ParameterGroups {
            positional_only: offset(regular.start),
            regular: offset(regular.len()),
            keyword_only: offset(ordinary.len() - regular.end),
            vararg,
            kwarg,
        };
        self.finish_parameters(range, start, groups)
    }

    pub(super) fn parameters_from_groups(
        &mut self,
        range: TextRange,
        [posonly, args, vararg, kwonly, kwarg]: [u64; 5],
    ) -> BuildId<crate::kind::Parameters> {
        let vararg = split_words(vararg)[0];
        let kwarg = split_words(kwarg)[0];
        let positional_only = split_words(posonly)[1];
        let keyword_only = split_words(kwonly)[1];
        if positional_only == 0 && keyword_only == 0 && vararg == 0 && kwarg == 0 {
            return self.push_regular_parameters(range, args);
        }

        let start = self.ast.extra.len();
        for list in [posonly, args] {
            self.ast
                .extra
                .extend_from_within(Interval::decode(list).indices());
        }
        if vararg != 0 {
            self.ast.extra.push(vararg);
        }
        self.ast
            .extra
            .extend_from_within(Interval::decode(kwonly).indices());
        if kwarg != 0 {
            self.ast.extra.push(kwarg);
        }
        self.finish_parameters(
            range,
            start,
            ParameterGroups {
                positional_only,
                regular: split_words(args)[1],
                keyword_only,
                vararg,
                kwarg,
            },
        )
    }

    #[inline]
    fn push_regular_parameters(
        &mut self,
        range: TextRange,
        items: u64,
    ) -> BuildId<crate::kind::Parameters> {
        let mut row = Row::new(NodeKind::Parameters, range);
        row.set_parameters_items(items);
        row.set_parameters_layout(ABSENT);
        self.push(row)
    }

    fn finish_parameters(
        &mut self,
        range: TextRange,
        start: usize,
        groups: ParameterGroups,
    ) -> BuildId<crate::kind::Parameters> {
        let mut row = Row::new(NodeKind::Parameters, range);
        let ParameterGroups {
            positional_only,
            regular,
            keyword_only,
            vararg,
            kwarg,
        } = groups;
        let extra = &mut self.ast.extra;
        let grouped_end = extra.len();
        let len = grouped_end - start;
        let ordinary_end = start + positional_only as usize + regular as usize;
        let keyword_start = ordinary_end + usize::from(vararg != 0);
        let keyword_end = keyword_start + keyword_only as usize;
        let rows = &self.ast.rows;
        let position = |slot: u32| rows[slot as usize].start();
        let misplaced_vararg = vararg != 0
            && (ordinary_end > start && position(extra[ordinary_end - 1]) > position(vararg)
                || keyword_end > keyword_start
                    && position(extra[keyword_start]) <= position(vararg));
        let last_ordinary = if keyword_end > keyword_start {
            Some(keyword_end - 1)
        } else if ordinary_end > start {
            Some(ordinary_end - 1)
        } else {
            None
        };
        let misplaced_kwarg = kwarg != 0
            && (last_ordinary.is_some_and(|p| position(extra[p]) > position(kwarg))
                || vararg != 0 && position(vararg) > position(kwarg));
        // Only recovered or synthetic parameters can need a projection in a different order.
        let ordered_start = if misplaced_vararg || misplaced_kwarg {
            let mut variadics = [vararg, kwarg];
            variadics.sort_by_key(|&id| {
                if id == 0 {
                    u32::MAX
                } else {
                    position(id).to_u32()
                }
            });
            let mut rest = variadics.into_iter().filter(|&id| id != 0).peekable();
            for i in (start..ordinary_end).chain(keyword_start..keyword_end) {
                let ordinary = extra[i];
                while let Some(&v) = rest.peek()
                    && position(v) < position(ordinary)
                {
                    extra.push(v);
                    rest.next();
                }
                extra.push(ordinary);
            }
            extra.extend(rest);
            grouped_end
        } else {
            start
        };
        row.set_parameters_items(Interval::new(ordered_start, len).encode());
        row.set_parameters_layout(offset(extra.len()));
        extra.extend([
            offset(start),
            positional_only,
            regular,
            keyword_only,
            vararg,
            kwarg,
        ]);
        self.push(row)
    }
}

impl<'a> Parameters<'a> {
    pub(super) fn has_only_regular_args(self) -> bool {
        self.syntax.row.parameters_layout() == ABSENT
    }

    fn parameter_layout(self) -> Option<&'a [u32; 6]> {
        self.syntax
            .ast
            .extra
            .get(self.syntax.row.parameters_layout() as usize..)?
            .first_chunk()
    }

    pub(super) fn parameter_group<const GROUP: usize>(self) -> u64 {
        let Some(&[start, posonly, regular, kwonly, vararg, _]) = self.parameter_layout() else {
            return if GROUP == 1 {
                self.syntax.row.parameters_items()
            } else {
                0
            };
        };
        let (start, len) = match GROUP {
            0 => (start, posonly),
            1 => (start + posonly, regular),
            _ => (start + posonly + regular + u32::from(vararg != 0), kwonly),
        };
        Interval::new(start as usize, len as usize).encode()
    }

    pub(super) fn variadic_parameter<const KEYWORDS: bool>(self) -> u64 {
        self.parameter_layout()
            .map_or(0, |layout| u64::from(layout[if KEYWORDS { 5 } else { 4 }]))
    }
}
