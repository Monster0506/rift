use super::runtime::{builtin_descriptor, BufferKindId, BufferPolicies, KindDescriptor};
use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct BufferKind {
    pub id: BufferKindId,
    pub descriptor: Arc<KindDescriptor>,
}

impl BufferKind {
    pub fn new(descriptor: Arc<KindDescriptor>) -> Self {
        Self {
            id: descriptor.id(),
            descriptor,
        }
    }

    pub fn for_builtin(id: BufferKindId) -> Self {
        Self {
            id,
            descriptor: builtin_descriptor(id),
        }
    }

    pub fn file() -> Self {
        Self::for_builtin(BufferKindId::FILE)
    }

    pub fn terminal() -> Self {
        Self::for_builtin(BufferKindId::TERMINAL)
    }

    pub fn directory() -> Self {
        Self::for_builtin(BufferKindId::DIRECTORY)
    }

    pub fn undotree() -> Self {
        Self::for_builtin(BufferKindId::UNDO_TREE)
    }

    pub fn messages() -> Self {
        Self::for_builtin(BufferKindId::MESSAGES)
    }

    pub fn clipboard() -> Self {
        Self::for_builtin(BufferKindId::CLIPBOARD)
    }

    pub fn clipboard_entry() -> Self {
        Self::for_builtin(BufferKindId::CLIPBOARD_ENTRY)
    }

    pub fn location_list() -> Self {
        Self::for_builtin(BufferKindId::LOCATION_LIST)
    }

    pub fn regions() -> Self {
        Self::for_builtin(BufferKindId::REGIONS)
    }

    pub fn scratch() -> Self {
        Self::for_builtin(BufferKindId::SCRATCH)
    }

    pub fn git_status() -> Self {
        Self::for_builtin(BufferKindId::GIT_STATUS)
    }

    pub fn git_commit_message() -> Self {
        Self::for_builtin(BufferKindId::GIT_COMMIT_MESSAGE)
    }

    pub fn git_blame() -> Self {
        Self::for_builtin(BufferKindId::GIT_BLAME)
    }

    pub fn git_log() -> Self {
        Self::for_builtin(BufferKindId::GIT_LOG)
    }

    pub fn git_rebase_todo() -> Self {
        Self::for_builtin(BufferKindId::GIT_REBASE_TODO)
    }

    pub fn buffer_list() -> Self {
        Self::for_builtin(BufferKindId::BUFFER_LIST)
    }

    pub fn undo_file_view() -> Self {
        Self::for_builtin(BufferKindId::UNDO_FILE_VIEW)
    }

    pub fn buffer_kind_id(&self) -> BufferKindId {
        self.id
    }

    pub fn id(&self) -> BufferKindId {
        self.id
    }

    pub fn descriptor(&self) -> &KindDescriptor {
        &self.descriptor
    }

    pub fn descriptor_arc(&self) -> Arc<KindDescriptor> {
        Arc::clone(&self.descriptor)
    }

    pub fn policies(&self) -> &BufferPolicies {
        self.descriptor.policies()
    }

    pub fn kind_str(&self) -> &str {
        self.descriptor.name()
    }

    pub fn display_name<'a>(
        &'a self,
        file_path: Option<&'a Path>,
        terminal_name: Option<&'a str>,
    ) -> Cow<'a, str> {
        self.descriptor
            .resolve_display_name(file_path, terminal_name)
    }
}

impl PartialEq for BufferKind {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for BufferKind {}

impl std::hash::Hash for BufferKind {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq<BufferKindId> for BufferKind {
    fn eq(&self, other: &BufferKindId) -> bool {
        self.id == *other
    }
}

impl PartialEq<BufferKind> for BufferKindId {
    fn eq(&self, other: &BufferKind) -> bool {
        *self == other.id
    }
}

impl std::fmt::Display for BufferKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.descriptor.name())
    }
}
