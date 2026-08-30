//! Guards: the conditions a controller decides by, and the tree they combine
//! into.
//!
//! A guard is declared against a [`crate::Domain`], not against one machine or
//! one feature, so the same node backs a [`crate::machine::Edge`] and a
//! [`crate::feature::Rule`]. That is what keeps "power is on" one definition
//! for a whole controller rather than one per table.

mod cond;
mod node;

pub use cond::Cond;
pub use node::{CondNode, Cx, Expr, Memo};

/// What to do when a guard evaluates to [`Cond::Unknown`].
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OnUnknown {
    /// Do not act when undecidable (fail-closed).
    Deny,
    /// Act when undecidable.
    Allow,
}

impl OnUnknown {
    /// Resolves a guard result into the yes/no answer a table row needs.
    ///
    /// - `cond`: the guard's result.
    ///
    /// Returns whether the row it guards may be taken.
    pub fn accepts(self, cond: Cond) -> bool {
        match cond {
            Cond::True => true,
            Cond::False => false,
            Cond::Unknown => self == Self::Allow,
        }
    }
}
