#[derive(Debug, Copy, Clone, is_macro::Is)]
pub enum ExecutionContext {
    /// The reference occurs in a runtime context.
    Runtime,
    /// The reference occurs in a typing-only context.
    Typing,
}
