mod ast;

use crate::sem::TypeReference;

/// A declaration-local modifier, separate from the wrapped value's identity.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WithTypmod<T> {
    value: T,
    arguments: Vec<String>,
    interpretation: TypmodInterpretation,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
enum TypmodInterpretation {
    Unspecified,
    Applied(i32),
    Unvalidated,
    Invalid(String),
}

impl TypmodInterpretation {
    const fn initial(has_arguments: bool) -> Self {
        if has_arguments {
            Self::Unvalidated
        } else {
            Self::Unspecified
        }
    }
    fn from_result(result: Result<i32, &'static str>) -> Self {
        match result {
            Ok(value) => Self::Applied(value),
            Err(message) => Self::Invalid(message.to_owned()),
        }
    }
    const fn typmod(&self) -> Option<i32> {
        if let Self::Applied(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    const fn is_validated(&self) -> bool {
        matches!(self, Self::Unspecified | Self::Applied(_))
    }
    fn validation_error(&self) -> Option<&str> {
        if let Self::Invalid(message) = self {
            Some(message)
        } else {
            None
        }
    }
}

impl<T> WithTypmod<T> {
    /// The wrapped value, without changing its identity or inference contract.
    pub const fn value(&self) -> &T {
        &self.value
    }
    /// Requested modifier arguments in SQL spelling and order.
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
    /// PostgreSQL's encoded modifier, including precision clamping where applicable.
    pub const fn typmod(&self) -> Option<i32> {
        self.interpretation.typmod()
    }
    /// Whether all requested modifiers have been interpreted successfully.
    pub const fn is_validated(&self) -> bool {
        self.interpretation.is_validated()
    }
    /// Why the owning declaration's modifier is invalid, when known.
    pub fn validation_error(&self) -> Option<&str> {
        self.interpretation.validation_error()
    }
    pub(crate) fn new(value: &T, arguments: &[String]) -> Self
    where
        T: Clone,
    {
        Self {
            value: value.clone(),
            arguments: arguments.to_vec(),
            interpretation: TypmodInterpretation::initial(!arguments.is_empty()),
        }
    }
}

impl WithTypmod<TypeReference> {
    pub(crate) fn interpret_postgres(&mut self, resolved: &TypeReference) {
        if self.arguments.is_empty() {
            return;
        }
        // Extension types may define arbitrary typmod input functions. Retain
        // their arguments without claiming that a builtin rule validates them.
        let Some(name) = resolved.full_name() else {
            return;
        };
        if name
            .schema()
            .is_none_or(|schema| schema.as_str() != "pg_catalog")
        {
            return;
        }
        let name = name.local_name();
        if !matches!(
            name.as_str(),
            "varchar"
                | "bpchar"
                | "numeric"
                | "bit"
                | "varbit"
                | "time"
                | "timetz"
                | "timestamp"
                | "timestamptz"
                | "interval"
                | "int2"
                | "int4"
                | "int8"
                | "text"
                | "bool"
                | "date"
                | "float4"
                | "float8"
                | "uuid"
                | "json"
                | "jsonb"
                | "bytea"
        ) {
            return;
        }
        self.interpretation = TypmodInterpretation::from_result(self.builtin_typmod(&name));
    }

    fn builtin_typmod(&self, name: &str) -> Result<i32, &'static str> {
        // PostgreSQL's ArrayGetIntegerTypmods accepts string/identifier spellings
        // of integers too. Keep the SQL spelling separately from its value.
        let values: Result<Vec<i32>, _> = self
            .arguments
            .iter()
            .map(|argument| Self::integer_argument(argument))
            .collect();
        let values = values.map_err(|_| "type modifiers must be integers")?;
        match (name, values.as_slice()) {
            ("varchar" | "bpchar", [length]) if (1..=10_485_760).contains(length) => Ok(length + 4),
            ("bit" | "varbit", [length]) if (1..=83_886_080).contains(length) => Ok(*length),
            ("numeric", [precision]) => Self::numeric(*precision, 0),
            ("numeric", [precision, scale]) => Self::numeric(*precision, *scale),
            ("time" | "timetz" | "timestamp" | "timestamptz", [precision]) if *precision >= 0 => {
                Ok((*precision).min(6))
            }
            ("interval", [range]) => Self::interval(*range, 65535),
            ("interval", [range, precision]) if *precision >= 0 => {
                Self::interval(*range, (*precision).min(6))
            }
            _ => Err("invalid type modifier arguments or range for declared type"),
        }
    }
    fn integer_argument(argument: &str) -> Result<i32, std::num::ParseIntError> {
        let argument = argument.trim();
        if let Some(inner) = argument.strip_prefix('\'')
            && let Some(inner) = inner.strip_suffix('\'')
        {
            inner.trim().parse()
        } else if let Some(inner) = argument.strip_prefix('"')
            && let Some(inner) = inner.strip_suffix('"')
        {
            inner.trim().parse()
        } else {
            argument.parse()
        }
    }
    fn numeric(precision: i32, scale: i32) -> Result<i32, &'static str> {
        if !(1..=1000).contains(&precision) || !(-1000..=1000).contains(&scale) {
            return Err("numeric precision must be 1..1000 and scale must be -1000..1000");
        }
        Ok(((precision << 16) | (scale & 0x7ff)) + 4)
    }
    fn interval(range: i32, precision: i32) -> Result<i32, &'static str> {
        if ![
            4, 2, 8, 1024, 2048, 4096, 6, 1032, 3080, 7176, 3072, 7168, 6144, 32767,
        ]
        .contains(&range)
        {
            return Err("invalid interval field range");
        }
        if range == 32767 && precision == 65535 {
            Ok(-1)
        } else {
            Ok((range << 16) | precision)
        }
    }
}
