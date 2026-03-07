#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Value {
    Bool(bool),
    UInt(u64),
    EnumVariant { label: String, index: u64 },
}
