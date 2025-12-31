macro_rules! impl_from_ref {
    ($($t:ty),* $(,)?) => {
        $(
            impl From<&$t> for $t
            where $t: Clone
            {
                fn from(value: &$t) -> Self {
                    value.clone()
                }
            }
        )*
    };
}
