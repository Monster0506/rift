//! Command and search history with prefix matching
//!
//! Provides in-memory history for command line and search inputs.
//! Supports prefix-based navigation (typing "o" then Up finds "open file").

/// Command history with prefix matching support
#[derive(Debug, Clone)]
pub struct CommandHistory {
    /// Stored history items (oldest first)
    items: Vec<String>,
    /// Maximum number of items to store
    max_size: usize,
    /// Current position during navigation (None = at "present"/new input)
    history_index: Option<usize>,
    /// The original input line before navigation started
    /// Used for prefix matching and restoring when returning to present
    original_line: Option<String>,
}

impl Default for CommandHistory {
    fn default() -> Self {
        Self::new(100)
    }
}

impl CommandHistory {
    /// Create a new command history with the specified maximum size
    #[must_use]
    pub fn new(max_size: usize) -> Self {
        Self {
            items: Vec::new(),
            max_size,
            history_index: None,
            original_line: None,
        }
    }

    /// Add a command to history
    ///
    /// - Skips empty commands
    /// - Skips consecutive duplicates
    /// - Resets navigation state
    pub fn add(&mut self, command: String) {
        // Skip empty commands
        if command.is_empty() {
            return;
        }

        // Skip consecutive duplicates
        if self.items.last() == Some(&command) {
            self.reset_navigation();
            return;
        }

        // Add to history
        self.items.push(command);

        // Trim to max size
        if self.items.len() > self.max_size {
            self.items.remove(0);
        }

        // Reset navigation state
        self.reset_navigation();
    }

    /// Start navigation if not already started
    ///
    /// Call this before prev/next to capture the current input line
    pub fn start_navigation(&mut self, current_line: String) {
        if self.original_line.is_none() {
            self.original_line = Some(current_line);
        }
    }

    /// Navigate to previous (older) matching history entry
    ///
    /// Returns the matching entry, or None if no match found
    pub fn prev_match(&mut self) -> Option<&str> {
        if self.items.is_empty() {
            return None;
        }

        let prefix = self.original_line.as_deref().unwrap_or("");

        // Determine starting position for search
        let start = match self.history_index {
            Some(idx) if idx > 0 => idx - 1,
            Some(_) => return None, // Already at oldest
            None => self.items.len() - 1,
        };

        // Search backwards for matching entry
        for i in (0..=start).rev() {
            if self.items[i].starts_with(prefix) {
                self.history_index = Some(i);
                return Some(&self.items[i]);
            }
        }

        None
    }

    /// Navigate to next (newer) matching history entry
    ///
    /// Returns the matching entry, or the original line if returning to present
    pub fn next_match(&mut self) -> Option<&str> {
        let idx = self.history_index?;

        let prefix = self.original_line.as_deref().unwrap_or("");

        // Search forwards for matching entry
        for i in (idx + 1)..self.items.len() {
            if self.items[i].starts_with(prefix) {
                self.history_index = Some(i);
                return Some(&self.items[i]);
            }
        }

        // No more matches - return to original line
        self.history_index = None;
        self.original_line.as_deref()
    }

    /// Reset navigation state
    ///
    /// Clears history_index and original_line, but keeps items
    pub fn reset_navigation(&mut self) {
        self.history_index = None;
        self.original_line = None;
    }

    /// Get the number of items in history
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if history is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
