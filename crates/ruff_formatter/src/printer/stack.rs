/// A school book stack. Allows adding, removing, and inspecting elements at the back.
pub(super) trait Stack<T> {
    /// Removes the last element if any and returns it
    fn pop(&mut self) -> Option<T>;

    /// Pushes a new element at the back
    fn push(&mut self, value: T);

    /// Returns the last element if any
    fn top(&self) -> Option<&T>;
}

impl<T> Stack<T> for Vec<T> {
    fn pop(&mut self) -> Option<T> {
        self.pop()
    }

    fn push(&mut self, value: T) {
        self.push(value);
    }

    fn top(&self) -> Option<&T> {
        self.last()
    }
}

/// A Stack that is stacked on top of another stack. Guarantees that the underlying stack remains unchanged.
#[derive(Debug, Clone)]
pub(super) struct StackedStack<'a, T> {
    /// The content of the original stack.
    original: std::slice::Iter<'a, T>,

    /// Items that have been pushed since the creation of this stack and aren't part of the `original` stack.
    stack: Vec<T>,
}

impl<'a, T> StackedStack<'a, T> {
    #[cfg(test)]
    pub(super) fn new(original: &'a [T]) -> Self {
        Self::with_vec(original, Vec::new())
    }

    /// Creates a new stack that uses `stack` for storing its elements.
    pub(super) fn with_vec(original: &'a [T], stack: Vec<T>) -> Self {
        Self {
            original: original.iter(),
            stack,
        }
    }

    /// Returns the underlying `stack` vector.
    pub(super) fn into_vec(self) -> Vec<T> {
        self.stack
    }
}

impl<T> Stack<T> for StackedStack<'_, T>
where
    T: Copy,
{
    fn pop(&mut self) -> Option<T> {
        self.stack
            .pop()
            .or_else(|| self.original.next_back().copied())
    }

    fn push(&mut self, value: T) {
        self.stack.push(value);
    }

    fn top(&self) -> Option<&T> {
        self.stack
            .last()
            .or_else(|| self.original.as_slice().last())
    }
}

#[cfg(test)]
mod tests {
    use crate::printer::stack::{Stack, StackedStack};

    #[test]
    fn restore_consumed_stack() {
        let original = vec![1, 2, 3];
        let mut restorable = StackedStack::new(&original);

        restorable.push(4);

        assert_eq!(restorable.pop(), Some(4));
        assert_eq!(restorable.pop(), Some(3));
        assert_eq!(restorable.pop(), Some(2));
        assert_eq!(restorable.pop(), Some(1));
        assert_eq!(restorable.pop(), None);

        assert_eq!(original, vec![1, 2, 3]);
    }

    #[test]
    fn restore_partially_consumed_stack() {
        let original = vec![1, 2, 3];
        let mut restorable = StackedStack::new(&original);

        restorable.push(4);

        assert_eq!(restorable.pop(), Some(4));
        assert_eq!(restorable.pop(), Some(3));
        assert_eq!(restorable.pop(), Some(2));
        restorable.push(5);
        restorable.push(6);
        restorable.push(7);

        assert_eq!(original, vec![1, 2, 3]);
    }

    #[test]
    fn restore_stack() {
        let original = vec![1, 2, 3];
        let mut restorable = StackedStack::new(&original);

        restorable.push(4);
        restorable.push(5);
        restorable.push(6);
        restorable.push(7);

        assert_eq!(restorable.pop(), Some(7));
        assert_eq!(restorable.pop(), Some(6));
        assert_eq!(restorable.pop(), Some(5));

        assert_eq!(original, vec![1, 2, 3]);
    }
}
