use crate::sem::{CreateIndex, IndexStorageParameter, problem::IndexProblemKind};

/// A validated B-tree storage parameter value.
#[derive(Clone, Debug, PartialEq)]
pub enum IndexStorageValue {
    /// Integral setting (fillfactor).
    Integer(i32),
    /// Boolean setting (deduplicate_items).
    Boolean(bool),
    /// Floating point setting (vacuum_cleanup_index_scale_factor).
    Real(f64),
}
impl IndexStorageValue {
    const fn integer(value: i32) -> Self {
        Self::Integer(value)
    }
    const fn boolean(value: bool) -> Self {
        Self::Boolean(value)
    }
    const fn real(value: f64) -> Self {
        Self::Real(value)
    }
    /// Integral value, if this is an integral setting.
    pub const fn as_integer(&self) -> Option<i32> {
        if let Self::Integer(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    /// Boolean value, if this is a Boolean setting.
    pub const fn as_boolean(&self) -> Option<bool> {
        if let Self::Boolean(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    /// Real value, if this is a real setting.
    pub const fn as_real(&self) -> Option<f64> {
        if let Self::Real(value) = self {
            Some(*value)
        } else {
            None
        }
    }

    fn parse(name: &str, value: &str) -> Result<Self, IndexProblemKind> {
        let invalid = IndexProblemKind::invalid_value();
        match name {
            "fillfactor" => {
                let value = integer_setting(value).ok_or(invalid)?;
                if !(10..=100).contains(&value) {
                    return Err(invalid);
                }
                Ok(Self::integer(value))
            }
            "deduplicate_items" => Ok(Self::boolean(boolean_setting(value).ok_or(invalid)?)),
            "vacuum_cleanup_index_scale_factor" => {
                let value = real_setting(value).ok_or(invalid)?;
                if !(0.0..=1e10).contains(&value) {
                    return Err(invalid);
                }
                Ok(Self::real(value))
            }
            _ => Err(IndexProblemKind::unknown_parameter()),
        }
    }
}

// PostgreSQL's integer reloptions use GUC integer parsing: base-prefixed
// integers and floating point input rounded to the nearest even integer.
fn radix_prefix(value: &str, prefix: [char; 2]) -> Option<&str> {
    value.strip_prefix('0')?.strip_prefix(prefix)
}
fn integer_setting(value: &str) -> Option<i32> {
    let value = value.trim_matches(|character: char| character.is_ascii_whitespace());
    let unsigned = value.strip_prefix('+').unwrap_or(value);
    let negative = unsigned.starts_with('-');
    let digits = unsigned.strip_prefix('-').unwrap_or(unsigned);
    let (digits, radix) = if let Some(hex) = radix_prefix(digits, ['x', 'X']) {
        (hex, 16)
    } else if let Some(binary) = radix_prefix(digits, ['b', 'B']) {
        (binary, 2)
    } else if digits.starts_with('0') {
        (digits, 8)
    } else {
        (digits, 10)
    };
    let remainder = digits.trim_start_matches(|character: char| character.is_digit(radix));
    if remainder.starts_with(['.', 'e', 'E']) {
        let number = real_setting(value)?.round_ties_even();
        if number < f64::from(i32::MIN) || number > f64::from(i32::MAX) {
            return None;
        }
        return Some(number as i32);
    }
    if !remainder.is_empty() {
        return None;
    }
    let number = i64::from_str_radix(digits, radix).ok()?;
    i32::try_from(if negative { -number } else { number }).ok()
}

// strtod accepts hexadecimal fractions as well as decimal input. Keep this
// conversion local to index settings; no server or platform C library is needed.
fn real_setting(value: &str) -> Option<f64> {
    let value = value.trim_matches(|character: char| character.is_ascii_whitespace());
    let unsigned = value.strip_prefix('+').unwrap_or(value);
    let negative = unsigned.starts_with('-');
    let unsigned = unsigned.strip_prefix('-').unwrap_or(unsigned);
    if unsigned.starts_with(['+', '-']) {
        return None;
    }
    let mut exact_hex_subnormal = false;
    let mut nonzero_hex = None;
    let number = if let Some(hex) = radix_prefix(unsigned, ['x', 'X']) {
        let (mantissa, exponent) = match hex.split_once(['p', 'P']) {
            Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().ok()?),
            None => (hex, 0),
        };
        let mut number = 0.0;
        let mut fraction = None;
        let mut count = 0;
        for character in mantissa.chars() {
            if character == '.' && fraction.is_none() {
                fraction = Some(1.0);
                continue;
            }
            let digit = f64::from(character.to_digit(16)?);
            count += 1;
            if let Some(place) = &mut fraction {
                *place /= 16.0;
                number += digit * *place;
            } else {
                number = number * 16.0 + digit;
            }
        }
        if count == 0 {
            return None;
        }
        nonzero_hex = Some(number != 0.0);
        let scaled = scale_binary(number, exponent);
        exact_hex_subnormal = exponent
            .checked_neg()
            .is_some_and(|inverse| scale_binary(scaled, inverse) == number);
        if negative { -scaled } else { scaled }
    } else {
        value.parse::<f64>().ok()?
    };
    // PostgreSQL rejects strtod range errors, including nonzero underflow.
    if !number.is_finite() || (number.is_subnormal() && !exact_hex_subnormal) {
        return None;
    }
    if number == 0.0 {
        let nonzero = nonzero_hex.unwrap_or_else(|| {
            unsigned
                .split(['e', 'E'])
                .next()
                .unwrap_or("")
                .chars()
                .any(|character| character != '0' && character != '.')
        });
        if nonzero {
            return None;
        }
    }
    Some(number)
}
fn scale_binary(mut number: f64, mut exponent: i32) -> f64 {
    // Chunking avoids overflowing the scale factor for a representable
    // subnormal result, such as 0x1p-1074.
    while exponent > 512 && number.is_finite() && number != 0.0 {
        number *= 2.0_f64.powi(512);
        exponent -= 512;
    }
    while exponent < -512 && number.is_finite() && number != 0.0 {
        number *= 2.0_f64.powi(-512);
        exponent += 512;
    }
    if number == 0.0 || !number.is_finite() {
        number
    } else {
        number * 2.0_f64.powi(exponent)
    }
}
fn boolean_setting(value: &str) -> Option<bool> {
    let value = value.to_ascii_lowercase();
    if value.is_empty() || value == "o" {
        return None;
    }
    if value == "1"
        || "true".starts_with(&value)
        || "yes".starts_with(&value)
        || "on".starts_with(&value)
    {
        Some(true)
    } else if value == "0"
        || "false".starts_with(&value)
        || "no".starts_with(&value)
        || "off".starts_with(&value)
    {
        Some(false)
    } else {
        None
    }
}

impl CreateIndex {
    pub(crate) fn apply_storage_parameter(
        &mut self,
        parameter: &IndexStorageParameter,
        reset: bool,
    ) -> Result<(), IndexProblemKind> {
        if reset {
            // RESET of an unknown name is a successful no-op in PostgreSQL.
            self.storage_parameters.remove(parameter.name());
        } else {
            let value =
                IndexStorageValue::parse(parameter.name(), parameter.value().unwrap_or("true"))?;
            self.storage_parameters
                .insert(parameter.name().to_owned(), value);
        }
        Ok(())
    }
}
