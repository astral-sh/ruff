use ruff_text_size::TextSize;
use rustc_hash::FxHashMap;

use super::{DocstringSectionKind, SectionBlock, SectionCandidate, SectionItem};
use crate::docstring::formats::rst;

pub(super) fn section_candidates(docstring: &rst::Docstring) -> Vec<SectionCandidate> {
    let mut sections = Vec::new();

    for field_list in docstring.field_lists() {
        if field_list.indent() != TextSize::default() {
            continue;
        }

        let Some(section) = section_block(field_list) else {
            continue;
        };

        let range = field_list.range();
        sections.push(SectionCandidate {
            range: range.start().to_usize()..range.end().to_usize(),
            block: section,
        });
    }

    sections
}

fn section_block(field_list: &rst::FieldList) -> Option<SectionBlock> {
    let fields = field_list.fields();
    let plan = RestFieldRenderPlan::from_fields(fields)?;
    let items = plan.items(fields);

    if items.is_empty() || items.iter().any(SectionItem::is_empty) {
        return None;
    }

    Some(SectionBlock::new(items))
}

/// Validates a reST field list and stores cross-field metadata needed while rendering.
struct RestFieldRenderPlan<'a> {
    parameter_types: SeparateTypeFields<'a>,
    attribute_types: SeparateTypeFields<'a>,
    return_type: Option<&'a str>,
    has_returns: bool,
}

impl<'a> RestFieldRenderPlan<'a> {
    fn from_fields(fields: &'a [rst::Field]) -> Option<Self> {
        let mut has_returns = false;
        let mut has_return_type = false;
        let mut parameter_types = SeparateTypeFields::default();
        let mut attribute_types = SeparateTypeFields::default();
        let mut return_type = None;

        for field in fields {
            match field {
                rst::Field::Parameter {
                    lookup_name, ty, ..
                } => {
                    parameter_types.record_value_field(lookup_name.as_str(), ty.is_some());
                }
                rst::Field::Attribute { name, ty, .. } => {
                    attribute_types.record_value_field(name.as_str(), ty.is_some());
                }
                rst::Field::Returns { .. } => {
                    has_returns = true;
                }
                rst::Field::Raises { .. } => {}
                rst::Field::ParameterType { lookup_name, ty } => {
                    parameter_types.record_type_field(lookup_name.as_str(), ty.as_str())?;
                }
                rst::Field::AttributeType { name, ty } => {
                    attribute_types.record_type_field(name.as_str(), ty.as_str())?;
                }
                rst::Field::ReturnType { ty } => {
                    if has_return_type {
                        return None;
                    }
                    has_return_type = true;
                    return_type = (!ty.is_empty()).then_some(ty.as_str());
                }
                rst::Field::Metadata => {}
                rst::Field::Unknown { .. } => return None,
            }
        }

        if !parameter_types.all_types_match_value_fields() {
            return None;
        }

        if !attribute_types.all_types_match_value_fields() {
            return None;
        }

        Some(Self {
            parameter_types,
            attribute_types,
            return_type,
            has_returns,
        })
    }

    fn items(&self, fields: &'a [rst::Field]) -> Vec<SectionItem> {
        let mut items = Vec::new();

        for field in fields {
            match field {
                rst::Field::Parameter {
                    display_name,
                    lookup_name,
                    ty,
                    description,
                } => items.push(SectionItem::new(
                    DocstringSectionKind::Parameters,
                    Some(display_name.as_str()),
                    ty.as_deref()
                        .or_else(|| self.parameter_types.get_non_empty(lookup_name.as_str())),
                    description,
                )),
                rst::Field::Attribute {
                    name,
                    ty,
                    description,
                } => items.push(SectionItem::new(
                    DocstringSectionKind::Attributes,
                    Some(name.as_str()),
                    ty.as_deref()
                        .or_else(|| self.attribute_types.get_non_empty(name.as_str())),
                    description,
                )),
                rst::Field::Returns { name, description } => items.push(SectionItem::new(
                    DocstringSectionKind::Returns,
                    name.as_deref(),
                    self.return_type,
                    description,
                )),
                rst::Field::Raises {
                    exception,
                    description,
                } => items.push(SectionItem::new(
                    DocstringSectionKind::Raises,
                    exception.as_deref(),
                    None,
                    description,
                )),
                rst::Field::ReturnType { .. } if !self.has_returns => {
                    if let Some(return_type) = self.return_type {
                        items.push(SectionItem::new(
                            DocstringSectionKind::Returns,
                            None,
                            Some(return_type),
                            "",
                        ));
                    }
                }
                rst::Field::ParameterType { .. }
                | rst::Field::AttributeType { .. }
                | rst::Field::ReturnType { .. }
                | rst::Field::Metadata
                | rst::Field::Unknown { .. } => {}
            }
        }

        items
    }
}

#[derive(Default)]
struct SeparateTypeFields<'a> {
    types: FxHashMap<&'a str, &'a str>,
    value_fields_accepting_type: FxHashMap<&'a str, bool>,
}

impl<'a> SeparateTypeFields<'a> {
    fn record_value_field(&mut self, name: &'a str, has_inline_type: bool) {
        self.value_fields_accepting_type
            .entry(name)
            .and_modify(|accepts_separate_type| *accepts_separate_type &= !has_inline_type)
            .or_insert(!has_inline_type);
    }

    fn record_type_field(&mut self, name: &'a str, ty: &'a str) -> Option<()> {
        self.types.insert(name, ty).is_none().then_some(())
    }

    fn all_types_match_value_fields(&self) -> bool {
        self.types.keys().all(|name| {
            self.value_fields_accepting_type
                .get(name)
                .copied()
                .unwrap_or(false)
        })
    }

    fn get_non_empty(&self, name: &str) -> Option<&'a str> {
        self.types.get(name).copied().filter(|ty| !ty.is_empty())
    }
}
