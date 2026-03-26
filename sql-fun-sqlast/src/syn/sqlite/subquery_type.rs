use turso_parser::ast::SubqueryType as SubqueryTypeInput;

/// exists subquery
#[derive(derive_getters::Getters)]
pub struct SubqueryTypeExists {
    result_reg: usize,
}

/// row value subquery
#[derive(derive_getters::Getters)]
pub struct SubqueryTypeRowValue {
    result_reg_start: usize,
    num_regs: usize,
}

/// In type subquery
#[derive(derive_getters::Getters)]
pub struct SubqueryTypeIn {
    cursor_id: usize,
    affinity_str: String,
}

/// sub-query type
pub enum SubqueryType {
    /// exists sub-query
    Exists(SubqueryTypeExists),
    /// row value
    RowValue(SubqueryTypeRowValue),
    /// In
    In(SubqueryTypeIn),
}

impl From<SubqueryTypeInput> for SubqueryType {
    fn from(value: SubqueryTypeInput) -> Self {
        match value {
            SubqueryTypeInput::Exists { result_reg } => {
                Self::Exists(SubqueryTypeExists { result_reg })
            }
            SubqueryTypeInput::RowValue {
                result_reg_start,
                num_regs,
            } => Self::RowValue(SubqueryTypeRowValue {
                result_reg_start,
                num_regs,
            }),
            SubqueryTypeInput::In {
                cursor_id,
                affinity_str,
            } => Self::In(SubqueryTypeIn {
                cursor_id,
                affinity_str: affinity_str.to_string(),
            }),
        }
    }
}
