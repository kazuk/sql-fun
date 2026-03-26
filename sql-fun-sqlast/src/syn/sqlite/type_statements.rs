use crate::syn::sqlite::Expr;

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

/// create type statement body
#[derive(derive_getters::Getters)]
pub struct CreateTypeBody {
    params: Vec<TypeParam>,
    base: String,
    encode: Option<Box<Expr>>,
    decode: Option<Box<Expr>>,
    operators: Vec<TypeOperator>,
}

impl From<CreateTypeBodyIn> for CreateTypeBody {
    fn from(value: CreateTypeBodyIn) -> Self {
        Self {
            params: value.params.into_iter().map(TypeParam::from).collect(),
            base: value.base,
            encode: value.encode.map(|e| Box::new(Expr::from(*e))),
            decode: value.decode.map(|e| Box::new(Expr::from(*e))),
            operators: value
                .operators
                .into_iter()
                .map(TypeOperator::from)
                .collect(),
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
