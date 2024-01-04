macro_rules! impl_kvlm_getter_single {
    ($($field:ident),+) => {
        $(pub fn $field(&self) -> Option<&String> {
            self.kvlm
                .get_single(stringify!($field))
        })+
    };
}
