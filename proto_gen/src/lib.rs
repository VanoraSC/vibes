//! Utilities for generating Rust code from Protocol Buffer definitions.
//!
//! The current implementation only contains scaffolding while the actual
//! code-generation pipeline is being built. The placeholder helpers make it
//! possible to exercise the testing infrastructure and keep the crate ready
//! for future expansion.

/// Adds two numbers together.
///
/// This helper exists solely as a placeholder while the Protocol Buffer code
/// generation functionality is implemented.
///
/// # Examples
///
/// ```
/// use proto_gen::add;
///
/// assert_eq!(add(2, 3), 5);
/// ```
pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the placeholder addition helper behaves correctly.
    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
