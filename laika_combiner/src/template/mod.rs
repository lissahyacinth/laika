use crate::template::error::TemplateError;
use crate::template::values::TemplateValue;
use serde::ser::SerializeMap;
use serde::Serialize;

pub(crate) mod error;
mod values;

pub(crate) type TemplateValues = Vec<TemplateValue>;
pub(crate) type TemplateBranch = Vec<(TemplateValues, TemplateNode)>;
#[derive(Debug, Clone)]
pub(crate) enum TemplateNode {
    Leaf(TemplateValues),
    // A branch is a KV, like a dict or a hashmap
    Branch(TemplateBranch),
}

impl RenderedTemplate {
    fn try_parse_leaf(
        leaf_node: TemplateValues,
        associated_value: &serde_json::Value,
    ) -> Result<Self, TemplateError> {
        Ok(RenderedTemplate::Leaf(
            leaf_node
                .into_iter()
                .map(|n| n.render(associated_value))
                .collect::<Result<Vec<String>, TemplateError>>()?
                .join(""),
        ))
    }

    fn try_parse(
        value: TemplateNode,
        associated_value: &serde_json::Value,
    ) -> Result<Self, TemplateError> {
        match value {
            TemplateNode::Leaf(leaf_node) => Ok(Self::try_parse_leaf(leaf_node, associated_value)?),
            TemplateNode::Branch(branch_node) => Ok(RenderedTemplate::Branch(
                branch_node
                    .into_iter()
                    .map(|(leaf_node, template_node)| {
                        Self::try_parse_leaf(leaf_node, associated_value).and_then(|x| {
                            Self::try_parse(template_node, associated_value).map(|y| (x, y))
                        })
                    })
                    .collect::<Result<Vec<(RenderedTemplate, RenderedTemplate)>, TemplateError>>(
                    )?,
            )),
        }
    }
}

pub(crate) enum RenderedTemplate {
    Leaf(String),
    Branch(Vec<(RenderedTemplate, RenderedTemplate)>),
}

impl Serialize for RenderedTemplate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            RenderedTemplate::Leaf(value) => serializer.serialize_str(value),
            RenderedTemplate::Branch(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

impl TemplateNode {
    fn from_key_value_pair(
        key: &serde_yaml::Value,
        value: &serde_yaml::Value,
    ) -> Result<(TemplateValues, TemplateNode), TemplateError> {
        if let Some(key) = key.as_str() {
            let key = TemplateValue::try_parse(key)?;
            let value = TemplateNode::from_value(value)?;
            Ok((key, value))
        } else {
            Err(TemplateError::KeyExpected)
        }
    }

    fn from_value(value: &serde_yaml::Value) -> Result<Self, TemplateError> {
        match value.as_mapping() {
            Some(mapping) => Ok(TemplateNode::Branch(
                mapping
                    .iter()
                    .map(|(key, value)| TemplateNode::from_key_value_pair(key, value))
                    .collect::<Result<Vec<(TemplateValues, TemplateNode)>, TemplateError>>()?,
            )),
            None => match value.as_str() {
                Some(value) => Ok(TemplateNode::Leaf(TemplateValue::try_parse(value)?)),
                None => Err(TemplateError::NoMappingFound),
            },
        }
    }
}

#[derive(Debug, Clone)]
/// A Template defines a YAML-compatible structure that can be rendered using an accompanying
/// data payload. i.e.;
///
/// Template;
/// ```yaml
///  metric: "conversion"
///  userId: "${{ userId }}"
///  paymentInfo:
///      timeToConvert: "${{ conversionTime }}"
///      revenue: "${{ purchaseAmount }}"
/// ```
/// Payload;
/// ```json
/// {
///     "userId": 1234,
///     "conversionTime: "00:00:00",
///     "purchaseAmount": 12.34
/// }
/// ```
pub struct Template {
    // Each branch can represent a KV, so a single root is sufficient
    root: TemplateNode,
}

impl Template {
    pub fn from_payload(value: &serde_yaml::Value) -> Result<Self, TemplateError> {
        Ok(Template {
            root: TemplateNode::from_value(value)?,
        })
    }

    /// Render a `crate::template::Template` into a format serializable with JSON
    pub(crate) fn render(
        self,
        associated_value: &serde_json::Value,
    ) -> Result<RenderedTemplate, TemplateError> {
        Ok(RenderedTemplate::try_parse(self.root, associated_value)?)
    }
}
