//! `BufferKind` descriptor-backed handle.
//!
//! A document directly holds its descriptor handle (`BufferKind { id, descriptor }`).
//! Generic editor behavior, policies, and display metadata are resolved through
//! the descriptor rather than an exhaustive data-carrying enum.

use super::runtime::{builtin_descriptor, BufferKindId, BufferPolicies, KindDescriptor};
use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;

/// Descriptor-backed buffer kind handle: interned `BufferKindId` plus its
/// immutable `KindDescriptor`, so hot paths avoid string hashing/matching.
#[derive(Debug, Clone)]
pub struct BufferKind {
    pub id: BufferKindId,
    pub descriptor: Arc<KindDescriptor>,
}

impl BufferKind {
    /// Creates a new `BufferKind` handle wrapping a descriptor.
    pub fn new(descriptor: Arc<KindDescriptor>) -> Self {
        Self {
            id: descriptor.id(),
            descriptor,
        }
    }

    /// Creates a `BufferKind` handle for a reserved built-in kind ID.
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

    /// Interned process-local ID for this kind.
    pub fn buffer_kind_id(&self) -> BufferKindId {
        self.id
    }

    /// Alias for [`buffer_kind_id`](Self::buffer_kind_id).
    pub fn id(&self) -> BufferKindId {
        self.id
    }

    /// Reference to the underlying descriptor.
    pub fn descriptor(&self) -> &KindDescriptor {
        &self.descriptor
    }

    /// Cloned `Arc` reference to the underlying descriptor.
    pub fn descriptor_arc(&self) -> Arc<KindDescriptor> {
        Arc::clone(&self.descriptor)
    }

    /// Reference to the policy bundle.
    pub fn policies(&self) -> &BufferPolicies {
        self.descriptor.policies()
    }

    /// Short lowercase string identifier for this kind (e.g. "file", "terminal").
    pub fn kind_str(&self) -> &str {
        self.descriptor.name()
    }

    /// Tab/UI label for this kind.
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
