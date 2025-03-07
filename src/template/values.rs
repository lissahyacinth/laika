use crate::template::error::TemplateError;
use crate::utils::extract_json::extract_json_field;
use serde_json::Value;

#[derive(PartialEq, Eq, Debug, Clone)]
pub(crate) struct TemplatedValue {
    prefix: Option<String>,
    template_fields: Vec<String>,
    postfix: Option<String>,
}

impl TemplatedValue {
    /// Render the template using the source JSON
    ///
    /// Fails if the relevant values cannot be found.

    fn format_json_value(value: &Value) -> String {
        match value {
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            Value::Array(arr) => {
                let items: Vec<String> = arr.iter()
                    .map(|v| Self::format_json_value(v))
                    .collect();
                format!("[{}]", items.join(", "))
            },
            Value::Object(obj) => Self::format_object(obj),
        }
    }

    fn format_object(obj: &serde_json::Map<String, Value>) -> String {
        let items: Vec<String> = obj.iter()
            .map(|(k, v)| format!("{}: {}", k, Self::format_json_value(v)))
            .collect();
        format!("{{{}}}", items.join(", "))
    }

    pub fn render(self, json: &Value) -> Result<String, TemplateError> {
        let extracted_element = Self::format_json_value(extract_json_field(json, &self.template_fields.join("."))
            .map_err(|e| TemplateError::RenderError(e.to_string()))?);
        tracing::debug!("Extracted element {}", extracted_element);
        Ok(format!(
            "{}{}{}",
            self.prefix.unwrap_or("".to_string()),
            extracted_element,
            self.postfix.unwrap_or("".to_string())
        ))
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum TemplateValue {
    Raw(String),
    Template(TemplatedValue),
}

impl TemplateValue {
    pub(crate) fn render(self, json: &Value) -> Result<String, TemplateError> {
        match self {
            TemplateValue::Raw(raw_string) => Ok(raw_string),
            TemplateValue::Template(templated_value) => templated_value.render(json),
        }
    }

    /// Attempt to parse `value` into one or multiple `TemplateValue`s.
    pub(crate) fn try_parse<T: Into<String>>(value: T) -> Result<Vec<Self>, TemplateError> {
        parse(lex(value.into().as_str()))
    }
}

#[derive(PartialEq, Eq, Debug)]
enum Token {
    Text(String),
    TemplateStart,
    TemplateIdentifier(String),
    TemplateDot,
    TemplateEnd,
}

fn lex(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current_text = String::new();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') && chars.clone().nth(1) == Some('{') {
            // We found a template start marker
            if !current_text.is_empty() {
                tokens.push(Token::Text(current_text));
                current_text = String::new();
            }

            // Consume the '{{' after the '$'
            chars.next();
            chars.next();
            tokens.push(Token::TemplateStart);

            // Collect whitespace before the identifier
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() {
                    chars.next();
                } else {
                    break;
                }
            }

            // Collect identifiers and dots
            let mut identifier = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_alphanumeric() || ch == '_' {
                    identifier.push(chars.next().unwrap());
                } else if ch == '.' {
                    if !identifier.is_empty() {
                        tokens.push(Token::TemplateIdentifier(identifier));
                        identifier = String::new();
                    }
                    chars.next();
                    tokens.push(Token::TemplateDot);
                } else if ch.is_whitespace() || ch == '}' {
                    break;
                } else {
                    // Unexpected character in identifier
                    identifier.push(chars.next().unwrap());
                }
            }

            if !identifier.is_empty() {
                tokens.push(Token::TemplateIdentifier(identifier));
            }

            // Consume whitespace before the closing brackets
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() {
                    chars.next();
                } else {
                    break;
                }
            }

            // Check for closing '}}' sequence
            if chars.next() == Some('}') && chars.next() == Some('}') {
                tokens.push(Token::TemplateEnd);
            } else {
                // Malformed template
                current_text.push_str("${{");
                // Continue normal text processing
            }
        } else {
            current_text.push(ch);
        }
    }

    if !current_text.is_empty() {
        tokens.push(Token::Text(current_text));
    }

    tokens
}

fn parse(mut tokens: Vec<Token>) -> Result<Vec<TemplateValue>, TemplateError> {
    let mut buffer = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i] {
            Token::Text(text) => {
                if !buffer.is_empty() {
                    if let Some(TemplateValue::Template(templated_value)) = buffer.last_mut() {
                        if templated_value.postfix.is_none() {
                            // This text is a postfix for the previous template
                            templated_value.postfix = Some(text.clone());
                            i += 1;
                            continue;
                        }
                    }
                }
                // Otherwise it's a typical text value
                buffer.push(TemplateValue::Raw(text.clone()));
                i += 1;
            }
            Token::TemplateStart => {
                let mut j = i + 1;
                let mut template_fields = Vec::new();
                let mut current_field = String::new();

                // Extract template fields
                while j < tokens.len() {
                    match &tokens[j] {
                        Token::TemplateIdentifier(id) => {
                            current_field = id.clone();
                            template_fields.push(current_field.clone());
                            j += 1;
                        }
                        Token::TemplateDot => {
                            j += 1;
                        }
                        Token::TemplateEnd => {
                            break;
                        }
                        _ => {
                            // Unexpected token
                            return Err(TemplateError::UnexpectedToken(j));
                        }
                    }
                }

                if j >= tokens.len() || !matches!(tokens[j], Token::TemplateEnd) {
                    return Err(TemplateError::UnclosedTemplate(i));
                }

                // Gosh this is a rubbish peek.
                let prefix = buffer
                    .pop()
                    .map(|possible_prefix| {
                        match possible_prefix {
                            TemplateValue::Raw(prefix) => Some(prefix),
                            TemplateValue::Template(t) => {
                                // We don't want this, put it back
                                buffer.push(TemplateValue::Template(t));
                                None
                            }
                        }
                    })
                    .flatten();

                buffer.push(TemplateValue::Template(TemplatedValue {
                    prefix,
                    template_fields,
                    postfix: None,
                }));

                // Skip to after the template end
                i = j + 1;
            }
            _ => {
                // Unexpected token at this position
                i += 1;
            }
        }
    }
    Ok(buffer)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::errors::LaikaResult;

    #[test]
    pub fn test_lex() {
        let input_targets = vec![
            (
                lex("raw_string"),
                vec![Token::Text("raw_string".to_string())],
            ),
            (
                lex("${{ raw_string }}"),
                vec![
                    Token::TemplateStart,
                    Token::TemplateIdentifier("raw_string".to_string()),
                    Token::TemplateEnd,
                ],
            ),
            (
                lex("Prefix ${{ raw_string }}"),
                vec![
                    Token::Text("Prefix ".to_string()),
                    Token::TemplateStart,
                    Token::TemplateIdentifier("raw_string".to_string()),
                    Token::TemplateEnd,
                ],
            ),
            (
                lex("${{ raw_string.sub_key }}"),
                vec![
                    Token::TemplateStart,
                    Token::TemplateIdentifier("raw_string".to_string()),
                    Token::TemplateDot,
                    Token::TemplateIdentifier("sub_key".to_string()),
                    Token::TemplateEnd,
                ],
            ),
            (
                lex("raw_string.sub_key"),
                vec![Token::Text("raw_string.sub_key".to_string())],
            ),
            (
                lex("Prefix ${{ raw_string }} SecondPrefix ${{ raw_string }}"),
                vec![
                    Token::Text("Prefix ".to_string()),
                    Token::TemplateStart,
                    Token::TemplateIdentifier("raw_string".to_string()),
                    Token::TemplateEnd,
                    Token::Text(" SecondPrefix ".to_string()),
                    Token::TemplateStart,
                    Token::TemplateIdentifier("raw_string".to_string()),
                    Token::TemplateEnd,
                ],
            ),
        ];
        for (output, expected_output) in input_targets {
            assert_eq!(output, expected_output)
        }
    }

    #[test]
    pub fn test_parse() -> Result<(), TemplateError> {
        let input_targets = vec![
            (
                parse(lex("raw_string")),
                vec![TemplateValue::Raw("raw_string".to_string())],
            ),
            (
                parse(lex("${{ raw_string }}")),
                vec![TemplateValue::Template(TemplatedValue {
                    prefix: None,
                    template_fields: vec!["raw_string".to_string()],
                    postfix: None,
                })],
            ),
            (
                parse(lex("${{ raw_string.sub_key }}")),
                vec![TemplateValue::Template(TemplatedValue {
                    prefix: None,
                    template_fields: vec!["raw_string".to_string(), "sub_key".to_string()],
                    postfix: None,
                })],
            ),
            (
                parse(lex("MyPrefix${{ raw_string.sub_key }}MyPostfix")),
                vec![TemplateValue::Template(TemplatedValue {
                    prefix: Some("MyPrefix".to_string()),
                    template_fields: vec!["raw_string".to_string(), "sub_key".to_string()],
                    postfix: Some("MyPostfix".to_string()),
                })],
            ),
            (
                parse(lex("MyPrefix${{ raw_string.sub_key }}${{ second_string }}")),
                vec![
                    TemplateValue::Template(TemplatedValue {
                        prefix: Some("MyPrefix".to_string()),
                        template_fields: vec!["raw_string".to_string(), "sub_key".to_string()],
                        postfix: None,
                    }),
                    TemplateValue::Template(TemplatedValue {
                        prefix: None,
                        template_fields: vec!["second_string".to_string()],
                        postfix: None,
                    }),
                ],
            ),
            (
                parse(lex("raw_string.sub_key")),
                vec![TemplateValue::Raw("raw_string.sub_key".to_string())],
            ),
        ];
        for (output, expected_output) in input_targets {
            assert_eq!(output?, expected_output)
        }
        Ok(())
    }
}
