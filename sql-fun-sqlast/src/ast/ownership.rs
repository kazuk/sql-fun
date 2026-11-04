use crate::ast::ObjectArgument;

impl ObjectArgument {
    pub(crate) fn is_output_only(&self) -> bool {
        self.mode.as_deref() == Some("OUT") || self.mode.as_deref() == Some("TABLE")
    }
}
