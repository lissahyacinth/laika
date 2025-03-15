use thiserror::Error;

#[derive(Error, Debug)]
pub enum TemplateError {
    #[error("Mapping Expected, but not found")]
    NoMappingFound,
    #[error("String expected in key slot, but no string found")]
    KeyExpected,
    #[error("Expected to find {0} matches, actually found {1}")]
    IncorrectMatchLength(usize, usize),
    #[error("No Tokens were found in parse")]
    NoTokensFound,
    #[error("Expected at most 3 tokens")]
    TooManyTokensFound,
    #[error("Tokens in an unexpected format - i.e. Prefix Prefix Token")]
    InvalidTokenArrangement,
    #[error("Unexpected token at position {0}")]
    UnexpectedToken(usize),
    #[error("Template not closed properly - expected a }} at position {0}")]
    UnclosedTemplate(usize),
    #[error("Could not render template from JSON due to {0}")]
    RenderError(String),
}
