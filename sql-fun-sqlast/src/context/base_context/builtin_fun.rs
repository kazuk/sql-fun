mod parse_util;

use self::parse_util::{deserialize_braced_list, deserialize_braced_list_opt};
use crate::{
    names::{ColumnName, FullName},
    sem::{
        AnalysisError, ColumnCollection, ColumnDefinition, CompositeFieldCollection,
        CreateCompositeType, CreateFunction, FunctionOverloadCollection, FunctionParam,
        OverloadVariant, TypeDefinition, TypeReference,
    },
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
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    returns_set: Option<bool>,
}

impl PgFunctionProto {
    fn preserves_view_rows(&self) -> bool {
        let (Some(kind), Some(returns_set)) = (&self.kind, self.returns_set) else {
            todo!("builtin function kind/returns_set extraction metadata");
        };
        kind == "f" && !returns_set
    }
    const fn returns_set(&self) -> bool {
        self.returns_set
            .expect("builtin function returns_set metadata")
    }
    fn matches_input_signature(&self, overload: &OverloadVariant) -> bool {
        let input: Vec<_> = overload
            .params()
            .iter()
            .filter(|parameter| parameter.is_input())
            .collect();
        self.args.len() == input.len()
            && self.args.iter().zip(input).all(|(name, parameter)| {
                parameter.get_type() == Some(&TypeReference::from_master_type_name(name))
            })
    }
}

#[cfg(test)]
mod test_function_properties {
    use crate::context::base_context::builtin_fun::PgFunctionProto;

    #[rstest::rstest]
    #[case::scalar("f", false, true)]
    #[case::aggregate("a", false, false)]
    #[case::window("w", false, false)]
    #[case::set("f", true, false)]
    fn retains_set_returning_independently_of_function_kind(
        #[case] kind: &str,
        #[case] returns_set: bool,
        #[case] preserves_rows: bool,
    ) {
        let definition = PgFunctionProto {
            args: vec!["int4".to_owned()],
            ret: "int4".to_owned(),
            is_strict: false,
            kind: Some(kind.to_owned()),
            returns_set: Some(returns_set),
        };
        assert_eq!(definition.returns_set(), returns_set);
        assert_eq!(definition.preserves_view_rows(), preserves_rows);
    }
}

#[derive(Debug, serde::Deserialize)]
enum PgArgMode {
    In,
    Out,
    Variadic,
}

impl PgArgMode {
    const fn is_output(&self) -> bool {
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
/// Builtin function definitions loaded from the selected master files.
pub struct PgBuiltInFunctions {
    function_by_name: HashMap<String, Vec<PgFunctionProto>>,
    function_details: HashMap<String, Vec<PgFunctionDetail>>,
}

impl PgBuiltInFunctions {
    #[cfg(test)]
    pub(crate) fn load_test_catalog(path: &Path, details_path: &Path) -> Self {
        Self::load_from(path, details_path).expect("test function catalog")
    }

    pub(super) fn preserves_view_rows(&self, name: &FullName, overload: &OverloadVariant) -> bool {
        self.property(name, overload, PgFunctionProto::preserves_view_rows)
    }
    pub(super) fn returns_set(&self, name: &FullName, overload: &OverloadVariant) -> bool {
        self.property(name, overload, PgFunctionProto::returns_set)
    }
    fn property(
        &self,
        name: &FullName,
        overload: &OverloadVariant,
        property: fn(&PgFunctionProto) -> bool,
    ) -> bool {
        let definitions = self
            .function_by_name
            .get(name.name().as_str())
            .expect("bound builtin function");
        // A uniform property also covers specialized polymorphic signatures.
        // Otherwise consult the exact producer signature.
        let mut common = None;
        let mut uniform = true;
        for definition in definitions {
            let preserves = property(definition);
            if common.is_some_and(|old| old != preserves) {
                uniform = false;
            }
            common = Some(preserves);
        }
        if uniform {
            return common.expect("builtin has overloads");
        }
        for definition in definitions {
            if definition.matches_input_signature(overload) {
                return property(definition);
            }
        }
        todo!("builtin function properties for selected polymorphic overload")
    }

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
            if let Some(details) = self.function_details.get(local_name)
                && let Some(details) = details
                    .iter()
                    .find(|detail| detail.matches_args(&overload.args))
            {
                let mut args = Vec::new();
                for arg in details.arg_iter() {
                    let arg_name = arg.arg_name.cloned();
                    let arg_type = Some(TypeReference::from_master_type_name(arg.arg_type));
                    args.push(arg.arg_mode.function_param(&arg_name, &arg_type));
                }

                let mut ret_type = TypeReference::from_master_type_name(&overload.ret);
                if details.out_args().next().is_some() {
                    let mut composite_fields = CompositeFieldCollection::default();
                    for arg in &args {
                        if let Some(field_def) = arg.field_definition() {
                            composite_fields.push(field_def);
                        }
                    }
                    let create_comp_type = CreateCompositeType::from_fields(&composite_fields);
                    let type_def = TypeDefinition::CompositeType(create_comp_type);
                    ret_type = TypeReference::dynamic(&type_def);
                }
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
        let col_defs = if let Some(details) = self.function_details.get(local_name)
            && let Some(details) = details.first()
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
mod test_declared_builtin_results {
    use crate::test_helpers::builtin_functions::functions_with_scalar_and_output_overloads;
    use crate::{
        names::{ColumnName, FullName},
        sem::{ColumnCollection, ColumnDefinition, FunctionParam, TypeReference},
    };

    #[rstest::rstest]
    #[case::input_only(0, "text", false, Some("value"))]
    #[case::variadic_scalar(1, "text", true, Some("pattern"))]
    #[case::no_details(2, "bool", false, None)]
    fn preserves_scalar_result_and_input_metadata(
        #[case] index: usize,
        #[case] declared_type: &str,
        #[case] variadic: bool,
        #[case] input_name: Option<&str>,
    ) {
        let functions = functions_with_scalar_and_output_overloads();
        let function = functions
            .find_function_by_name(&FullName::with_schema("pg_catalog", "probe"))
            .unwrap();
        let overload = function.overloads().get_at(index).unwrap();
        assert_eq!(
            overload.ret_type(),
            &Some(TypeReference::from_master_type_name(declared_type))
        );
        let input_type =
            TypeReference::from_master_type_name(if variadic { "text" } else { declared_type });
        let mut expected = vec![FunctionParam::new_input_param(
            &input_name.map(String::from),
            &Some(input_type),
            &None,
        )];
        if variadic {
            expected.push(FunctionParam::new_variadig_param(
                &Some(String::from("values")),
                &Some(TypeReference::from_master_type_name("any")),
            ));
        }
        assert_eq!(overload.params(), &expected);
    }

    #[rstest::rstest]
    #[case::two_output_fields(3, "int4", &[ ("label", "text"), ("count", "int4") ])]
    #[case::different_output_overload(4, "int8", &[ ("total", "int8") ])]
    fn preserves_output_fields_per_overload(
        #[case] index: usize,
        #[case] input_type: &str,
        #[case] output_fields: &[(&str, &str)],
    ) {
        let functions = functions_with_scalar_and_output_overloads();
        let function = functions
            .find_function_by_name(&FullName::with_schema("pg_catalog", "probe"))
            .unwrap();
        let overload = function.overloads().get_at(index).unwrap();
        let result = overload.ret_type().as_ref().unwrap();
        assert!(result.is_dynamic());
        let columns = result
            .type_definition()
            .unwrap()
            .get_composite_type_columns();
        let mut expected = vec![FunctionParam::new_input_param(
            &Some(String::from("value")),
            &Some(TypeReference::from_master_type_name(input_type)),
            &None,
        )];
        let mut expected_columns = Vec::new();
        for (name, ty) in output_fields {
            let ty = TypeReference::from_master_type_name(ty);
            expected_columns.push(ColumnDefinition::new(
                &Some(ColumnName::from(*name)),
                Some(&ty),
                Some(false),
                &[],
            ));
            expected.push(FunctionParam::new_output_param(
                &Some(String::from(*name)),
                &Some(ty),
            ));
        }
        assert_eq!(columns, ColumnCollection::new(expected_columns));
        assert_eq!(overload.params(), &expected);
    }
}

#[cfg(test)]
mod test_builtin_function_details {
    use std::path::PathBuf;

    use crate::names::FullName;

    use crate::context::base_context::builtin_fun::PgBuiltInFunctions;

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
