use std::ops::{Deref, DerefMut};

use common::serde::de::DeserializeOwned;

use super::prelude::*;

/// Implements patching for `Box`
///
/// All methods simply pass throught to the boxed value.
impl<Type: Patchable> Patchable for Box<Type>
where
    Type: Clone + DeserializeOwned + Send + 'static,
{
    fn diff(&self, other: &Self, differ: &mut Differ) {
        self.deref().diff(other, differ)
    }

    fn apply_add(&mut self, address: &mut Address, value: &Value) -> Result<()> {
        self.deref_mut().apply_add(address, value)
    }

    fn apply_remove(&mut self, address: &mut Address, items: usize) -> Result<()> {
        self.deref_mut().apply_remove(address, items)
    }

    fn apply_replace(&mut self, address: &mut Address, items: usize, value: &Value) -> Result<()> {
        self.deref_mut().apply_replace(address, items, value)
    }

    fn apply_move(&mut self, from: &mut Address, items: usize, to: &mut Address) -> Result<()> {
        self.deref_mut().apply_move(from, items, to)
    }

    fn apply_transform(&mut self, address: &mut Address, from: &str, to: &str) -> Result<()> {
        self.deref_mut().apply_transform(address, from, to)
    }

    /// Cast a [`Value`] to a `Box<Type>` instance
    ///
    /// If the value is a `Box<Type>`: then just use it. Otherwise, attempt to convert
    /// to and instance of `Type` and then box it.
    fn from_value(value: &Value) -> Result<Self> {
        let instance = if let Some(value) = value.downcast_ref::<Self>() {
            value.clone()
        } else {
            Box::new(Type::from_value(value)?)
        };
        Ok(instance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{apply_new, diff};
    use stencila_schema::CodeBlock;
    use test_utils::{assert_json_eq, assert_json_is};

    #[test]
    fn basic() -> Result<()> {
        // Add, remove, replace
        let a = Box::new("abcd".to_string());
        let b = Box::new("eacp".to_string());
        let patch = diff(&a, &b);
        assert_json_is!(
            patch.ops,
            [
                {"type": "Add", "address": [0], "value": "e", "length": 1},
                {"type": "Remove", "address": [2], "items": 1},
                {"type": "Replace", "address": [3], "items": 1, "value": "p", "length": 1}
            ]
        );
        assert_json_is!(apply_new(&a, &patch)?, b);

        Ok(())
    }

    // Regression, found by proptest, related to bug in `from_value`
    #[test]
    fn regression_1() -> Result<()> {
        let a = CodeBlock::default();
        let b = CodeBlock {
            programming_language: Some(Box::new("a".to_string())),
            ..Default::default()
        };
        let patch = diff(&a, &b);
        assert_json_is!(patch.ops, [
            { "type": "Add", "address": ["programmingLanguage"], "value": "a", "length": 1 },
        ]);
        assert_json_eq!(apply_new(&a, &patch)?, b);

        Ok(())
    }
}
