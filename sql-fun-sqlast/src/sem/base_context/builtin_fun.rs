mod parse_util;

use self::parse_util::{deserialize_braced_list, deserialize_braced_list_opt};
use crate::sem::{
    AnalysisError, ColumnCollection, ColumnDefinition, ColumnName, CreateCompositType,
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

#[derive(Debug, serde::Deserialize, PartialEq)]
enum PgArgMode {
    In,
    Out,
    Variadic,
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
    pub fn arg_length(&self) -> usize {
        assert_eq!(self.allargtypes.len(), self.argmodes.len());
        if let Some(argnames) = &self.argnames {
            assert_eq!(self.allargtypes.len(), argnames.len())
        }
        self.allargtypes.len()
    }

    pub fn arg_nth(&self, index: usize) -> Option<PgFunctionArgRef<'_>> {
        if index >= self.allargtypes.len() {
            return None;
        };
        let arg_name = self.argnames.as_ref().map(|names| &names[index]);
        Some(PgFunctionArgRef {
            arg_type: &self.allargtypes[index],
            arg_mode: &self.argmodes[index],
            arg_name,
        })
    }

    pub fn arg_iter(&self) -> PgFunctionArgIter<'_> {
        PgFunctionArgIter {
            source: self,
            index: 0,
        }
    }

    pub fn out_args(&self) -> impl Iterator<Item = PgFunctionArgRef<'_>> {
        self.arg_iter().filter(|a| a.arg_mode == &PgArgMode::Out)
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
    function_details: HashMap<String, PgFunctionDetail>,
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
        let function_details: HashMap<String, PgFunctionDetail> = function_details
            .into_iter()
            .map(|v| (v.name.clone(), v))
            .collect();
        Ok(Self {
            function_by_name,
            function_details,
        })
    }

    #[tracing::instrument(skip(self))]
    pub(super) fn find_function_by_name(&self, name: &FullName) -> Option<CreateFunction> {
        let local_name = name.name().as_str();
        let func_def = self.function_by_name.get(local_name)?;

        let mut variants = FunctionOverloadCollection::default();
        if let Some(details) = self.function_details.get(local_name) {
            let mut args = Vec::new();
            for arg in details.arg_iter() {
                match arg.arg_mode {
                    PgArgMode::In => {
                        let arg_name = arg.arg_name.cloned();
                        let arg_type = Some(TypeReference::from_master_type_name(arg.arg_type));

                        args.push(FunctionParam::new_input_param(&arg_name, &arg_type, &None))
                    }
                    PgArgMode::Out => {
                        let arg_name = arg.arg_name.cloned();
                        let arg_type = Some(TypeReference::from_master_type_name(arg.arg_type));
                        args.push(FunctionParam::new_output_param(&arg_name, &arg_type))
                    }
                    PgArgMode::Variadic => {
                        let arg_name = arg.arg_name.cloned();
                        let arg_type = Some(TypeReference::from_master_type_name(arg.arg_type));
                        args.push(FunctionParam::new_variadig_param(&arg_name, &arg_type))
                    }
                }
            }

            let mut composite_fields = CompositeFieldCollection::default();
            for arg in &args {
                if let Some(field_def) = arg.field_definition() {
                    composite_fields.push(field_def);
                }
            }
            let create_comp_type = CreateCompositType::from_fields(&composite_fields);
            let type_def = TypeDefinition::CompositType(create_comp_type);
            let ret_type = TypeReference::dynamic(&type_def);

            variants.push(&OverloadVariant::new(&Some(ret_type), &args, true, false))
        } else {
            for overload in func_def {
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
        let col_defs = if let Some(details) = self.function_details.get(local_name) {
            let mut columns = Vec::new();
            for out_arg in details.out_args() {
                let col_type = TypeReference::from_master_type_name(out_arg.arg_type);

                columns.push(ColumnDefinition::new(
                    &out_arg.arg_name.map(|v| ColumnName::from(v.to_string())),
                    Some(&col_type),
                    None,
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
