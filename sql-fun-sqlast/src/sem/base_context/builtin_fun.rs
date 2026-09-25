mod parse_util;

use self::parse_util::{deserialize_braced_list, deserialize_braced_list_opt};
use crate::sem::{
    AnalysisError, ColumnCollection, ColumnDefinition, ColumnName, CreateCompositeType,
    CreateFunction, FullName, FunctionOverloadCollection, FunctionParam, OverloadVariant,
    TypeDefinition, TypeReference, create_comp_type::CompositeFieldCollection,
};
use std::{collections::HashMap, fs::File, io::BufReader, path::Path};

#[derive(Debug, serde::Deserialize)]
struct PgFunctionDefinition {
    name: String,
    overloads: Vec<PgOverloadDefinition>,
}

#[expect(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct PgOverloadDefinition {
    name: String,
    #[serde(flatten)]
    proto: PgFunctionProto,
}

#[derive(Debug, serde::Deserialize)]
struct PgFunctionProto {
    args: Vec<String>,
    ret: String,
    is_strict: bool,
}

#[derive(Debug, serde::Deserialize)]
enum PgArgMode {
    In,
    Out,
    Variadic,
}

impl PgArgMode {
    fn is_output(&self) -> bool {
        matches!(self, Self::Out)
    }

    fn function_param(
        &self,
        name: &Option<String>,
        param_type: &Option<TypeReference>,
    ) -> FunctionParam {
        match self {
            Self::In => FunctionParam::new_input_param(name, param_type, &None),
            Self::Out => FunctionParam::new_output_param(name, param_type),
            Self::Variadic => FunctionParam::new_variadig_param(name, param_type),
        }
    }
}

impl std::str::FromStr for PgArgMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "i" => Self::In,
            "o" => Self::Out,
            "v" => Self::Variadic,
            s => return Err(String::from(s)),
        })
    }
}

#[derive(Debug, serde::Deserialize)]
struct PgFunctionDetail {
    name: String,
    #[serde(deserialize_with = "deserialize_braced_list")]
    allargtypes: Vec<String>,
    #[serde(deserialize_with = "deserialize_braced_list")]
    argmodes: Vec<PgArgMode>,
    #[serde(deserialize_with = "deserialize_braced_list_opt")]
    argnames: Option<Vec<String>>,
}

struct PgFunctionArgRef<'a> {
    arg_type: &'a String,
    arg_mode: &'a PgArgMode,
    arg_name: Option<&'a String>,
}

impl PgFunctionDetail {
    fn matches_args(&self, args: &[String]) -> bool {
        self.arg_iter()
            .filter(|arg| !arg.arg_mode.is_output())
            .map(|arg| arg.arg_type)
            .eq(args.iter())
    }

    pub fn arg_length(&self) -> usize {
        assert_eq!(self.allargtypes.len(), self.argmodes.len());
        if let Some(argnames) = &self.argnames {
            assert_eq!(self.allargtypes.len(), argnames.len());
        }
        self.allargtypes.len()
    }

    pub fn arg_nth(&self, index: usize) -> Option<PgFunctionArgRef<'_>> {
        if index >= self.allargtypes.len() {
            return None;
        }
        let arg_name = self.argnames.as_ref().map(|names| &names[index]);
        Some(PgFunctionArgRef {
            arg_type: &self.allargtypes[index],
            arg_mode: &self.argmodes[index],
            arg_name,
        })
    }

    pub const fn arg_iter(&self) -> PgFunctionArgIter<'_> {
        PgFunctionArgIter {
            source: self,
            index: 0,
        }
    }

    pub fn out_args(&self) -> impl Iterator<Item = PgFunctionArgRef<'_>> {
        self.arg_iter().filter(|a| a.arg_mode.is_output())
    }
}

struct PgFunctionArgIter<'a> {
    source: &'a PgFunctionDetail,
    index: usize,
}

impl<'a> Iterator for PgFunctionArgIter<'a> {
    type Item = PgFunctionArgRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.source.arg_length() {
            let result = self.source.arg_nth(self.index);
            self.index += 1;
            result
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct PgBuiltInFunctions {
    function_by_name: HashMap<String, Vec<PgFunctionProto>>,
    function_details: HashMap<String, Vec<PgFunctionDetail>>,
}

impl PgBuiltInFunctions {
    pub(super) fn load_from<P: AsRef<Path>, P2: AsRef<Path>>(
        path: P,
        details_path: P2,
    ) -> Result<Self, AnalysisError> {
        let definitions: Vec<PgFunctionDefinition> =
            serde_json::from_reader(BufReader::new(File::open(path)?))?;
        let function_by_name: HashMap<String, Vec<PgFunctionProto>> = definitions
            .into_iter()
            .map(|v| (v.name, v.overloads.into_iter().map(|v| v.proto).collect()))
            .collect();
        let function_details: Vec<PgFunctionDetail> =
            serde_json::from_reader(BufReader::new(File::open(details_path)?))?;
        let mut details_by_name: HashMap<String, Vec<PgFunctionDetail>> = HashMap::new();
        for detail in function_details {
            details_by_name
                .entry(detail.name.clone())
                .or_default()
                .push(detail);
        }
        Ok(Self {
            function_by_name,
            function_details: details_by_name,
        })
    }

    #[tracing::instrument(skip(self))]
    pub(super) fn find_function_by_name(&self, name: &FullName) -> Option<CreateFunction> {
        let local_name = name.name().as_str();
        let func_def = self.function_by_name.get(local_name)?;

        let mut variants = FunctionOverloadCollection::default();
        for overload in func_def {
            if let Some(details) = self.function_details.get(local_name).and_then(|details| {
                details
                    .iter()
                    .find(|detail| detail.matches_args(&overload.args))
            }) {
                let mut args = Vec::new();
                for arg in details.arg_iter() {
                    let arg_name = arg.arg_name.cloned();
                    let arg_type = Some(TypeReference::from_master_type_name(arg.arg_type));
                    args.push(arg.arg_mode.function_param(&arg_name, &arg_type));
                }

                let mut composite_fields = CompositeFieldCollection::default();
                for arg in &args {
                    if let Some(field_def) = arg.field_definition() {
                        composite_fields.push(field_def);
                    }
                }
                let create_comp_type = CreateCompositeType::from_fields(&composite_fields);
                let type_def = TypeDefinition::CompositeType(create_comp_type);
                let ret_type = TypeReference::dynamic(&type_def);
                variants.push(&OverloadVariant::new(
                    &Some(ret_type),
                    &args,
                    overload.is_strict,
                    false,
                ));
            } else {
                let ret_type = Some(TypeReference::from_master_type_name(&overload.ret));
                let params = overload
                    .args
                    .iter()
                    .map(|a| {
                        FunctionParam::new_input_param(
                            &None,
                            &Some(TypeReference::from_master_type_name(a)),
                            &None,
                        )
                    })
                    .collect();
                variants.push(&OverloadVariant::new(
                    &ret_type,
                    &params,
                    overload.is_strict,
                    false,
                ));
            }
        }
        let col_defs = if let Some(details) = self
            .function_details
            .get(local_name)
            .and_then(|v| v.first())
        {
            let mut columns = Vec::new();
            for out_arg in details.out_args() {
                let col_type = TypeReference::from_master_type_name(out_arg.arg_type);

                columns.push(ColumnDefinition::new(
                    &out_arg.arg_name.map(|v| ColumnName::from(v.clone())),
                    Some(&col_type),
                    None,
                    &[],
                ))
            }

            let column_definitions = ColumnCollection::new(columns);
            Some(column_definitions)
        } else {
            None
        };

        let func = CreateFunction::new(
            &FullName::with_schema("pg_catalog", local_name),
            &variants,
            &col_defs,
        );

        Some(func)
    }
}

#[cfg(test)]
mod test_builtin_function_details {
    use std::path::PathBuf;

    use crate::sem::FullName;

    use super::PgBuiltInFunctions;

    #[test]
    fn keeps_each_record_returning_overload() {
        for version in ["17", "18"] {
            let base = PathBuf::from(env!("SQL_FUN_HOME"))
                .join("postgres")
                .join(version);
            let functions = PgBuiltInFunctions::load_from(
                base.join("functions.json"),
                base.join("func_detail.json"),
            )
            .expect("committed builtin artifacts load");

            for (name, parameter_counts) in [
                ("pg_stat_file", [7, 8]),
                ("pg_ls_tmpdir", [3, 4]),
                ("ts_debug", [7, 8]),
            ] {
                let function = functions
                    .find_function_by_name(&FullName::with_schema("pg_catalog", name))
                    .expect("builtin function exists");
                let variants: Vec<_> = function.overloads().iter().collect();
                assert_eq!(variants.len(), parameter_counts.len(), "{version}: {name}");

                for parameter_count in parameter_counts {
                    assert!(
                        variants.iter().any(|variant| {
                            variant
                                .ret_type()
                                .as_ref()
                                .is_some_and(|ty| ty.is_dynamic())
                                && variant.params().len() == parameter_count
                        }),
                        "{version}: {name} with {parameter_count} parameters"
                    );
                }
            }
        }
    }
}
