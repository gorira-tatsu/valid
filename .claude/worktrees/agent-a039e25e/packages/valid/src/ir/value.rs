#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Value {
    Bool(bool),
    UInt(u64),
    String(String),
    EnumVariant {
        label: String,
        index: u64,
    },
    PairVariant {
        left_label: String,
        left_index: u64,
        right_label: String,
        right_index: u64,
    },
}
