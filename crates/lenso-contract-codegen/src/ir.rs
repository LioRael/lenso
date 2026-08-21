//! Language-neutral contract representation consumed by binding backends.

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ContractIr {
    pub(super) capability_id: String,
    pub(super) version: String,
    pub(super) portable: bool,
    pub(super) cross_lane_transfer: bool,
    pub(super) operations: Vec<OperationIr>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OperationIr {
    pub(super) name: String,
    pub(super) interaction: String,
    pub(super) request: TypeIr,
    pub(super) response: TypeIr,
    pub(super) domain_errors: Vec<ErrorVariantIr>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum TypeIr {
    Any,
    String,
    Int64,
    Uint64,
    Bytes,
    Timestamp,
    Duration,
    Integer,
    Number,
    Boolean,
    Null,
    Enum(Vec<String>),
    Array(Box<Self>),
    Object {
        fields: Vec<FieldIr>,
        additional: ObjectAdditionalIr,
    },
    Nullable(Box<Self>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FieldIr {
    pub(super) name: String,
    pub(super) required: bool,
    pub(super) sensitive: bool,
    pub(super) ty: TypeIr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ObjectAdditionalIr {
    Closed,
    Any,
    Typed(Box<TypeIr>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ErrorVariantIr {
    pub(super) code: String,
    pub(super) name: String,
    pub(super) structured: bool,
    pub(super) payload: Option<TypeIr>,
    pub(super) payload_required: bool,
}

impl TypeIr {
    pub(super) fn is_nullable(&self) -> bool {
        matches!(self, Self::Nullable(_))
    }

    pub(super) fn non_null(&self) -> &Self {
        match self {
            Self::Nullable(inner) => inner,
            _ => self,
        }
    }
}
