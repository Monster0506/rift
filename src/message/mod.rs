/// Messages from the Command Line component
pub enum CommandLineMessage {
    ExecuteCommand(String),
    ExecuteSearch(String),
    CancelMode,                // Used for closing modals, clearing command line
    RequestCompletion(String), // Tab pressed; String is current command line content
}
