use crate::sem::TypeReference;

/// Read PostgreSQL transforms by canonical data type and language.
pub trait TransformationInfoRead {
    /// Whether the effective catalog contains a transform for this identity.
    fn transformation_exists(&self, _data_type: &TypeReference, _language: &str) -> bool {
        todo!("PostgreSQL transform lookup (#1187)")
    }
}

/// Register PostgreSQL transforms in a writable catalog layer.
pub trait TransformationInfoWrite: Sized {
    /// Register the identity of a validated transform.
    fn register_transformation(self, _data_type: &TypeReference, _language: &str) -> Self {
        todo!("PostgreSQL transform catalog application (#1187)")
    }
}
