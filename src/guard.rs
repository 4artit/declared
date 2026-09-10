//! Guards: the conditions a controller decides by, and the tree they combine
//! into.
//!
//! A guard is declared against a [`crate::Domain`], not against one feature, so
//! the same node backs a [`crate::feature::Rule`] of any of them. That is what
//! keeps "power is on" one definition for a whole controller rather than one
//! per table.
//!
//! # Node names are unique per domain
//!
//! [`Memo`] keys on [`CondNode::name`], so two node types answering to one name
//! is a defect: the second inherits the first's result without running.
//! [`crate::verify::duplicate_node_names`] enforces it — run it in a test.
//!
//! Where the nodes are declared does not matter. Keeping a domain's guards in
//! one module gets the names for free, since Rust rejects the second
//! declaration.

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
