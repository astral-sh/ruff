use ruff_text_size::TextSize;
use rustc_hash::FxHashMap;

use super::{DocstringSectionKind, SectionBlock, SectionCandidate, SectionItem};
use crate::docstring::formats::rst;

pub(super) fn section_candidates(field_lists: &rst::Docstring) -> Vec<SectionCandidate> {
    let mut sections = Vec::new();

    for field_list in field_lists.field_lists() {
        if field_list.indent() != TextSize::default() {
            continue;
        }

        let range = field_list.range();

        let Some(section) = section_block(field_list) else {
            continue;
        };

        sections.push(SectionCandidate {
            range: range.start().to_usize()..range.end().to_usize(),
            block: section,
        });
    }

    sections
}

fn section_block(field_list: &rst::FieldList) -> Option<SectionBlock> {
    let plan = RestFieldRenderPlan::from_fields(field_list.fields())?;
    let items = plan.items(field_list.fields());
    items
        .iter()
        .all(|item| !item.is_empty())
        .then(|| SectionBlock::new(items))
}

/// Validates a reST field list and stores cross-field metadata needed while rendering.
struct RestFieldRenderPlan<'a> {
    parameter_types: FxHashMap<&'a str, &'a str>,
    attribute_types: FxHashMap<&'a str, &'a str>,
    return_type: Option<&'a str>,
    has_returns: bool,
}

impl<'a> RestFieldRenderPlan<'a> {
    fn from_fields(fields: &'a [rst::Field]) -> Option<Self> {
        let mut has_rendered_field = false;
        let mut has_returns = false;
        let mut has_return_type = false;
        let mut parameters: FxHashMap<&'a str, TypedFieldRenderState> = FxHashMap::default();
        let mut attributes: FxHashMap<&'a str, TypedFieldRenderState> = FxHashMap::default();
        let mut parameter_types: FxHashMap<&'a str, &'a str> = FxHashMap::default();
        let mut attribute_types: FxHashMap<&'a str, &'a str> = FxHashMap::default();
        let mut return_type = None;

        for field in fields {
            match field {
                rst::Field::Parameter {
                    lookup_name, ty, ..
                } => {
                    has_rendered_field = true;
                    parameters
                        .entry(lookup_name.as_str())
                        .or_default()
                        .record_field(ty.is_some());
                }
                rst::Field::Attribute { name, ty, .. } => {
                    has_rendered_field = true;
                    attributes
                        .entry(name.as_str())
                        .or_default()
                        .record_field(ty.is_some());
                }
                rst::Field::Returns { .. } => {
                    has_rendered_field = true;
                    has_returns = true;
                }
                rst::Field::Raises { .. } => {
                    has_rendered_field = true;
                }
                rst::Field::ParameterType { lookup_name, ty } => {
                    if parameter_types
                        .insert(lookup_name.as_str(), ty.as_str())
                        .is_some()
                    {
                        return None;
                    }
                }
                rst::Field::AttributeType { name, ty } => {
                    if attribute_types.insert(name.as_str(), ty.as_str()).is_some() {
                        return None;
                    }
                }
                rst::Field::ReturnType { ty } => {
                    if has_return_type {
                        return None;
                    }
                    has_return_type = true;
                    return_type = (!ty.is_empty()).then_some(ty.as_str());
                    has_rendered_field |= return_type.is_some();
                }
                rst::Field::Metadata => {}
                rst::Field::Unknown { .. } => return None,
            }
        }

        if parameter_types.keys().any(|name| {
            !parameters
                .get(name)
                .is_some_and(TypedFieldRenderState::accepts_separate_type)
        }) {
            return None;
        }

        if attribute_types.keys().any(|name| {
            !attributes
                .get(name)
                .is_some_and(TypedFieldRenderState::accepts_separate_type)
        }) {
            return None;
        }

        if !has_rendered_field {
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
                    Some(display_name.to_string()),
                    ty.as_deref()
                        .or_else(|| {
                            self.parameter_types
                                .get(lookup_name.as_str())
                                .copied()
                                .filter(|ty| !ty.is_empty())
                        })
                        .map(str::to_string),
                    description.clone(),
                )),
                rst::Field::Attribute {
                    name,
                    ty,
                    description,
                } => items.push(SectionItem::new(
                    DocstringSectionKind::Attributes,
                    Some(name.to_string()),
                    ty.as_deref()
                        .or_else(|| {
                            self.attribute_types
                                .get(name.as_str())
                                .copied()
                                .filter(|ty| !ty.is_empty())
                        })
                        .map(str::to_string),
                    description.clone(),
                )),
                rst::Field::Returns { name, description } => items.push(SectionItem::new(
                    DocstringSectionKind::Returns,
                    name.as_ref().map(ToString::to_string),
                    self.return_type.map(str::to_string),
                    description.clone(),
                )),
                rst::Field::Raises {
                    exception,
                    description,
                } => items.push(SectionItem::new(
                    DocstringSectionKind::Raises,
                    exception.as_ref().map(ToString::to_string),
                    None,
                    description.clone(),
                )),
                rst::Field::ReturnType { .. } if !self.has_returns => {
                    if let Some(return_type) = self.return_type {
                        items.push(SectionItem::new(
                            DocstringSectionKind::Returns,
                            None,
                            Some(return_type.to_string()),
                            String::new(),
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
struct TypedFieldRenderState {
    has_untyped_field: bool,
    has_inline_typed_field: bool,
}

impl TypedFieldRenderState {
    fn record_field(&mut self, has_inline_type: bool) {
        if has_inline_type {
            self.has_inline_typed_field = true;
        } else {
            self.has_untyped_field = true;
        }
    }

    fn accepts_separate_type(&self) -> bool {
        self.has_untyped_field && !self.has_inline_typed_field
    }
}
