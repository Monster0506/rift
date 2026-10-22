use super::runtime::{builtin_descriptor, BufferKindId, BufferPolicies, KindDescriptor};
use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct BufferKind {
    pub(crate) id: BufferKindId,
    pub(crate) descriptor: Arc<KindDescriptor>,
}

impl BufferKind {
    #[must_use]
    pub(crate) fn new(descriptor: Arc<KindDescriptor>) -> Self {
        Self {
            id: descriptor.id(),
            descriptor,
        }
    }

    #[must_use]
    pub(crate) fn for_builtin(id: BufferKindId) -> Self {
        Self {
            id,
            descriptor: builtin_descriptor(id),
        }
    }

    #[must_use]
    pub(crate) fn file() -> Self {
        Self::for_builtin(BufferKindId::FILE)
    }

    #[must_use]
    pub(crate) fn terminal() -> Self {
        Self::for_builtin(BufferKindId::TERMINAL)
    }

    #[must_use]
    pub(crate) fn directory() -> Self {
        Self::for_builtin(BufferKindId::DIRECTORY)
    }

    #[must_use]
    pub(crate) fn undotree() -> Self {
        Self::for_builtin(BufferKindId::UNDO_TREE)
    }

    #[must_use]
    pub(crate) fn messages() -> Self {
        Self::for_builtin(BufferKindId::MESSAGES)
    }

    #[must_use]
    pub(crate) fn clipboard() -> Self {
        Self::for_builtin(BufferKindId::CLIPBOARD)
    }

    #[must_use]
    pub(crate) fn scratch() -> Self {
        Self::for_builtin(BufferKindId::SCRATCH)
    }

    #[must_use]
    pub(crate) fn git_status() -> Self {
        Self::for_builtin(BufferKindId::GIT_STATUS)
    }

    #[must_use]
    pub(crate) fn git_commit_message() -> Self {
        Self::for_builtin(BufferKindId::GIT_COMMIT_MESSAGE)
    }

    #[must_use]
    pub(crate) fn git_blame() -> Self {
        Self::for_builtin(BufferKindId::GIT_BLAME)
    }

    #[must_use]
    pub(crate) fn git_log() -> Self {
        Self::for_builtin(BufferKindId::GIT_LOG)
    }

    #[must_use]
    pub(crate) fn git_rebase_todo() -> Self {
        Self::for_builtin(BufferKindId::GIT_REBASE_TODO)
    }

    #[must_use]
    pub(crate) fn buffer_list() -> Self {
        Self::for_builtin(BufferKindId::BUFFER_LIST)
    }

    #[must_use]
    pub(crate) fn undo_file_view() -> Self {
        Self::for_builtin(BufferKindId::UNDO_FILE_VIEW)
    }

    #[must_use]
    pub(crate) const fn id(&self) -> BufferKindId {
        self.id
    }

    #[must_use]
    pub(crate) fn descriptor(&self) -> &KindDescriptor {
        &self.descriptor
    }

    #[must_use]
    pub(crate) fn policies(&self) -> &BufferPolicies {
        self.descriptor.policies()
    }

    #[must_use]
    pub(crate) fn kind_str(&self) -> &str {
        self.descriptor.name()
    }

    #[must_use]
    pub(crate) fn display_name<'a>(
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
