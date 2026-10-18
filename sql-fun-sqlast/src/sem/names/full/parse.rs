use super::FullName;
use crate::sem::AnalysisError;

impl FullName {
    pub(super) fn parse_components(input: &str) -> Result<Self, AnalysisError> {
        let invalid = || AnalysisError::type_name_format(&input);
        let mut chars = input.chars().peekable();
        let mut parts = Vec::new();
        loop {
            let mut part = String::new();
            if chars.peek() == Some(&'"') {
                chars.next();
                loop {
                    match chars.next() {
                        Some('"') if chars.peek() == Some(&'"') => {
                            chars.next();
                            part.push('"');
                        }
                        Some('"') => break,
                        Some(character) => part.push(character),
                        None => return Err(invalid()),
                    }
                }
            } else {
                while let Some(&character) = chars.peek() {
                    if character == '.' {
                        break;
                    }
                    if !(character.is_alphanumeric() || matches!(character, '_' | '$')) {
                        return Err(invalid());
                    }
                    part.push(character);
                    chars.next();
                }
                if part.is_empty() {
                    return Err(invalid());
                }
            }
            parts.push(part);
            match chars.next() {
                None => break,
                Some('.') if parts.len() < 3 => (),
                _ => return Err(invalid()),
            }
        }
        match parts.as_slice() {
            [local] => Ok(Self::from_local_name(local)),
            [schema, local] => Ok(Self::with_schema(schema, local)),
            [catalog, schema, local] => Ok(Self::with_catalog(catalog, schema, local)),
            _ => Err(invalid()),
        }
    }
}
