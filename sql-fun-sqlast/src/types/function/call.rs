//! Call-shaped filtering without casts, polymorphic binding or ranking.

use super::{FunctionOverloads, FunctionReadError, FunctionSignature, ParameterMode};
use crate::sem::ColumnName;
use crate::types::array::ArrayType;
use crate::types::{Candidates, TypeDefinition, TypeReference};

/// One syntactically supplied argument, in call order, with its type candidates.
#[derive(Debug, Clone)]
pub struct CallArgument {
    name: Option<ColumnName>,
    candidates: TypeReference,
    variadic: bool,
}

impl CallArgument {
    /// A positional argument. Candidate membership uses `same_type`.
    pub fn positional(candidates: &dyn TypeDefinition) -> Self {
        Self {
            name: None,
            candidates: candidates.clone_box(),
            variadic: false,
        }
    }
    /// A named argument, using an already normalized logical SQL name.
    pub fn named(name: &ColumnName, candidates: &dyn TypeDefinition) -> Self {
        Self {
            name: Some(name.clone()),
            candidates: candidates.clone_box(),
            variadic: false,
        }
    }
    /// Mark this final argument as an explicit SQL `VARIADIC` array argument.
    pub const fn with_variadic(mut self) -> Self {
        self.variadic = true;
        self
    }
}

#[derive(Debug, Clone)]
struct MatchedSignature {
    signature: FunctionSignature,
    arguments: Vec<TypeReference>,
}

/// Surviving whole signatures and the expected types of syntactically supplied values.
///
/// Mapping is retained per signature: named reordering and expanded variadic inputs
/// never turn into declaration indices. Provider order, duplicates and scopes survive.
#[derive(Debug, Clone)]
pub struct FunctionCallMatches(Vec<MatchedSignature>);

impl FunctionCallMatches {
    /// Whole surviving signatures, suitable for further declaration-level reads.
    pub fn overloads(&self) -> FunctionOverloads {
        let signatures: Vec<_> = self.0.iter().map(|m| m.signature.clone()).collect();
        FunctionOverloads::new(&signatures)
    }
    /// Whether no candidate can accept the supplied call.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Expected types at a zero-based syntactic call position, including named values.
    /// Expanded variadic values project element types; explicit VARIADIC projects arrays.
    /// Missing positions and empty matches produce `Never`.
    pub fn argument_candidates(&self, index: usize) -> TypeReference {
        let values: Vec<_> = self
            .0
            .iter()
            .filter_map(|m| m.arguments.get(index).cloned())
            .collect();
        Candidates::from_candidates(&values)
    }
    /// Further constrain one syntactic value while retaining correlated signatures.
    pub fn filter_argument(&self, index: usize, candidates: &dyn TypeDefinition) -> Self {
        Self(
            self.0
                .iter()
                .filter(|m| {
                    m.arguments
                        .get(index)
                        .is_some_and(|t| FunctionOverloads::member(candidates, t.as_ref()))
                })
                .cloned()
                .collect(),
        )
    }
    /// Result candidates of surviving whole signatures; procedures contribute nothing.
    pub fn result_candidates(&self) -> TypeReference {
        self.overloads().result_candidates()
    }
    /// Further constrain results while retaining the original call mapping.
    pub fn filter_result(&self, candidates: &dyn TypeDefinition) -> Self {
        Self(
            self.0
                .iter()
                .filter(|m| {
                    m.signature
                        .result()
                        .result_type()
                        .is_some_and(|r| FunctionOverloads::member(candidates, r))
                })
                .cloned()
                .collect(),
        )
    }
}

impl FunctionOverloads {
    /// Filter complete calls, accounting for names, defaults and variadic expansion.
    ///
    /// Arguments are in syntax order: positional values precede named values, and
    /// explicit VARIADIC is last. Duplicate assignment, unknown names, missing
    /// required inputs, excess inputs and invalid syntax shapes retain no signature.
    /// OUT/TABLE parameters never consume values. An omitted variadic parameter
    /// requires a declared default (use explicit VARIADIC for an empty array).
    /// Ambiguous declaration names return an error rather than choosing a parameter.
    ///
    /// Type matching uses direct `same_type` membership, as in `filter_argument`.
    /// Supply cast-expanded candidates explicitly when needed. This operation does
    /// not solve polymorphic constraints, rank survivors or select an overload.
    pub fn match_call(
        &self,
        arguments: &[CallArgument],
    ) -> Result<FunctionCallMatches, FunctionReadError> {
        let mut matches = Vec::new();
        for signature in &self.0 {
            if let Some(types) = signature.match_arguments(arguments)? {
                matches.push(MatchedSignature {
                    signature: signature.clone(),
                    arguments: types,
                });
            }
        }
        Ok(FunctionCallMatches(matches))
    }
}

impl FunctionSignature {
    fn match_arguments(
        &self,
        arguments: &[CallArgument],
    ) -> Result<Option<Vec<TypeReference>>, FunctionReadError> {
        let inputs: Vec<_> = self
            .parameters
            .iter()
            .filter(|p| p.mode().accepts_input())
            .collect();
        let mut used = vec![false; inputs.len()];
        let mut expected = Vec::new();
        let mut position = 0;
        let mut named = false;
        for (index, argument) in arguments.iter().enumerate() {
            if argument.variadic && index + 1 != arguments.len() {
                return Ok(None);
            }
            let target = if let Some(name) = &argument.name {
                named = true;
                let Some(parameter) = self.argument_by_name(name)? else {
                    return Ok(None);
                };
                let Some(target) = inputs.iter().position(|p| std::ptr::eq(*p, parameter)) else {
                    return Ok(None);
                };
                target
            } else {
                if named {
                    return Ok(None);
                }
                position
            };
            let Some(parameter) = inputs.get(target) else {
                return Ok(None);
            };
            let variadic = parameter.mode() == ParameterMode::variadic();
            if argument.variadic && !variadic {
                return Ok(None);
            }
            let expanded = variadic && !argument.variadic;
            // Named variadic parameters require explicit array passing in PostgreSQL.
            if expanded && argument.name.is_some() {
                return Ok(None);
            }
            if used[target] && (!expanded || argument.name.is_some()) {
                return Ok(None);
            }
            let type_ = if expanded {
                let Some(array) = parameter
                    .parameter_type()
                    .as_any()
                    .downcast_ref::<ArrayType>()
                else {
                    return Ok(None);
                };
                array.element_type()
            } else {
                parameter.parameter_type()
            };
            if !FunctionOverloads::member(argument.candidates.as_ref(), type_) {
                return Ok(None);
            }
            used[target] = true;
            expected.push(type_.clone_box());
            if !named && !expanded {
                position += 1;
            }
        }
        if inputs
            .iter()
            .zip(used)
            .any(|(p, used)| !used && p.default_expression().is_none())
        {
            return Ok(None);
        }
        Ok(Some(expected))
    }
}

#[cfg(test)]
mod tests;
