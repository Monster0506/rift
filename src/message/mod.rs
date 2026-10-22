/// Messages from the Command Line component
pub(crate) enum CommandLineMessage {
    RequestCompletion(String), // Tab pressed; String is current command line content
}
