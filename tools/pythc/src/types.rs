use crate::ast::TypeName;

pub use pythos_shared::pyth_tig::types::PythType;

pub fn ast_type_to_pyth_type(ty: TypeName) -> PythType {
    match ty {
        TypeName::Bool => PythType::Bool,
        TypeName::U64 => PythType::U64,
        TypeName::I64 => PythType::I64,
        TypeName::Bytes => PythType::Bytes,
        TypeName::Utf8 => PythType::Utf8,
        TypeName::ObjectId => PythType::ObjectId,
        TypeName::RevisionId => PythType::RevisionId,
        TypeName::TaskId => PythType::TaskId,
        TypeName::ProposalId => PythType::ProposalId,
        TypeName::Capability => PythType::Capability,
        TypeName::ErrorCode => PythType::ErrorCode,
        TypeName::Unit => PythType::Unit,
    }
}

pub fn is_integer_like_type(ty: PythType) -> bool {
    matches!(
        ty,
        PythType::U64
            | PythType::I64
            | PythType::ObjectId
            | PythType::RevisionId
            | PythType::TaskId
            | PythType::ProposalId
            | PythType::ErrorCode
    )
}
