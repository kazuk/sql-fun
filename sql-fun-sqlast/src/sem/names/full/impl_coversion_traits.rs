use crate::{
    sem::{AnalysisError, CatalogName, LocalName, SchemaName},
    syn::{ListOpt, NodeListOpt, RangeVar, TypeName},
};

use super::FullName;

impl TryFrom<NodeListOpt> for FullName {
    type Error = AnalysisError;

    fn try_from(value: NodeListOpt) -> Result<Self, Self::Error> {
        let Some(name_elements) = value.map(|v| v.as_string()) else {
            AnalysisError::raise_unexpected_none("node_list")?
        };
        let elements: Vec<String> = name_elements.iter().map(|v| v.get_sval()).collect();
        match elements.len() {
            3 => Ok(Self {
                catalog: Some(CatalogName::from(&elements[0])),
                schema: Some(SchemaName::from(&elements[1])),
                name: LocalName::from(&elements[2]),
            }),
            2 => Ok(Self {
                catalog: None,
                schema: Some(SchemaName::from(&elements[0])),
                name: LocalName::from(&elements[1]),
            }),
            1 => Ok(Self {
                catalog: None,
                schema: None,
                name: LocalName::from(&elements[0]),
            }),
            _ => Err(AnalysisError::type_name_format(&elements)),
        }
    }
}

impl TryFrom<RangeVar> for FullName {
    type Error = AnalysisError;

    fn try_from(value: RangeVar) -> Result<Self, Self::Error> {
        let catalog = value.get_catalogname();
        let schema = value.get_schemaname();
        let name = value.get_relname();
        Ok(Self {
            catalog: {
                if catalog.is_empty() {
                    None
                } else {
                    Some(CatalogName::from(&catalog))
                }
            },
            schema: {
                if schema.is_empty() {
                    None
                } else {
                    Some(SchemaName::from(&schema))
                }
            },
            name: LocalName::from(&name),
        })
    }
}

impl TryFrom<TypeName> for FullName {
    type Error = AnalysisError;

    fn try_from(value: TypeName) -> Result<Self, Self::Error> {
        let Some(names) = value.get_names().map(|v| v.as_string()) else {
            AnalysisError::raise_unexpected_none("type_name.names")?
        };
        let names: Vec<String> = names.iter().map(|v| v.get_sval()).collect();

        match names.len() {
            3 => Ok(Self {
                catalog: Some(CatalogName::from(&names[0])),
                schema: Some(SchemaName::from(&names[1])),
                name: LocalName::from(&names[2]),
            }),
            2 => Ok(Self {
                catalog: None,
                schema: Some(SchemaName::from(&names[0])),
                name: LocalName::from(&names[1]),
            }),
            1 => Ok(Self {
                catalog: None,
                schema: None,
                name: LocalName::from(&names[0]),
            }),
            _ => Err(AnalysisError::type_name_format(&names)),
        }
    }
}

impl std::str::FromStr for FullName {
    type Err = AnalysisError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() == 3 {
            Ok(Self {
                catalog: Some(CatalogName::from(parts[0])),
                schema: Some(SchemaName::from(parts[1])),
                name: LocalName::from(parts[2]),
            })
        } else if parts.len() == 2 {
            Ok(Self {
                catalog: None,
                schema: Some(SchemaName::from(parts[0])),
                name: LocalName::from(parts[1]),
            })
        } else {
            Ok(Self {
                catalog: None,
                schema: None,
                name: LocalName::from(parts[0]),
            })
        }
    }
}
