use crate::names::FullName;

impl FullName {
    pub(crate) fn parse_components(input: &str) -> Option<Self> {
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
                        None => return None,
                    }
                }
            } else {
                while let Some(&character) = chars.peek() {
                    if character == '.' {
                        break;
                    }
                    if !(character.is_alphanumeric() || matches!(character, '_' | '$')) {
                        return None;
                    }
                    part.push(character);
                    chars.next();
                }
                if part.is_empty() {
                    return None;
                }
            }
            parts.push(part);
            match chars.next() {
                None => break,
                Some('.') if parts.len() < 3 => (),
                _ => return None,
            }
        }
        match parts.as_slice() {
            [local] => Some(Self::from_local_name(local)),
            [schema, local] => Some(Self::with_schema(schema, local)),
            [catalog, schema, local] => Some(Self::with_catalog(catalog, schema, local)),
            _ => None,
        }
    }
}
