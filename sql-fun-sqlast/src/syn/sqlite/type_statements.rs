use crate::syn::sqlite::{Expr, Name, Type};

/// create type statement
#[derive(derive_getters::Getters)]
pub struct CreateTypeStatement {
    if_not_exists: bool,
    type_name: String,
    body: CreateTypeBody,
}

impl CreateTypeStatement {
    /// create instance
    pub fn new(if_not_exists: bool, type_name: String, body: CreateTypeBody) -> Self {
        Self {
            if_not_exists,
            type_name,
            body,
        }
    }
}

use turso_parser::ast::CreateTypeBody as CreateTypeBodyIn;

#[derive(derive_getters::Getters)]
pub struct CreateCustomType {
    params: Vec<TypeParam>,
    base: String,
    encode: Option<Box<Expr>>,
    decode: Option<Box<Expr>>,
    operators: Vec<TypeOperator>,
    default: Option<Box<Expr>>,
}

impl CreateCustomType {
    pub fn new(
        params: Vec<TypeParam>,
        base: &str,
        encode: Option<Expr>,
        decode: Option<Expr>,
        operators: Vec<TypeOperator>,
        default: Option<Expr>,
    ) -> Self {
        Self {
            params,
            base: String::from(base),
            encode: encode.map(Box::new),
            decode: decode.map(Box::new),
            operators,
            default: default.map(Box::new),
        }
    }
}

/// create type statement body
pub enum CreateTypeBody {
    /// create a custom type
    CustomType(CreateCustomType),
    /// create a struct type
    Struct(Vec<TypeField>),
    /// create a union type
    Union(Vec<TypeField>),
}

impl From<CreateTypeBodyIn> for CreateTypeBody {
    fn from(value: CreateTypeBodyIn) -> Self {
        match value {
            CreateTypeBodyIn::CustomType {
                params,
                base,
                encode,
                decode,
                operators,
                default,
            } => Self::CustomType(CreateCustomType::new(
                params.into_iter().map(TypeParam::from).collect(),
                &base,
                encode.map(|v| Expr::from(*v)),
                decode.map(|v| Expr::from(*v)),
                operators.into_iter().map(TypeOperator::from).collect(),
                default.map(|v| Expr::from(*v)),
            )),
            CreateTypeBodyIn::Struct(type_fields) => {
                Self::Struct(type_fields.into_iter().map(TypeField::from).collect())
            }
            CreateTypeBodyIn::Union(type_fields) => {
                Self::Union(type_fields.into_iter().map(TypeField::from).collect())
            }
        }
    }
}

use turso_parser::ast::TypeField as TypeFieldIn;

#[derive(derive_getters::Getters)]
pub struct TypeField {
    name: Name,
    field_type: Type,
}

impl From<TypeFieldIn> for TypeField {
    fn from(value: TypeFieldIn) -> Self {
        Self {
            name: Name::from(value.name),
            field_type: Type::from(value.field_type),
        }
    }
}

use turso_parser::ast::TypeParam as TypeParamIn;

/// type parameter
#[derive(derive_getters::Getters)]
pub struct TypeParam {
    name: String,
    ty: Option<String>,
}

impl From<TypeParamIn> for TypeParam {
    fn from(value: TypeParamIn) -> Self {
        Self {
            name: value.name,
            ty: value.ty,
        }
    }
}

use turso_parser::ast::TypeOperator as TypeOperatorIn;

/// type operator
#[derive(derive_getters::Getters)]
pub struct TypeOperator {
    op: String,
    func_name: Option<String>,
}

impl From<TypeOperatorIn> for TypeOperator {
    fn from(value: TypeOperatorIn) -> Self {
        Self {
            op: value.op,
            func_name: value.func_name,
        }
    }
}

/// drop type statement
#[derive(derive_getters::Getters)]
pub struct DropTypeStatement {
    if_exists: bool,
    type_name: String,
}

impl DropTypeStatement {
    /// create instance
    pub fn new(if_exists: bool, type_name: String) -> Self {
        Self {
            if_exists,
            type_name,
        }
    }
}
