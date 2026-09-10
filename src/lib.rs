#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
// The core is allocation-free: dispatch, guard evaluation and the tables it
// reads touch no heap. `alloc` is for what reports on a controller rather than
// runs it — `render`'s documents and `verify`'s findings. Under `cfg(test)` the
// harness needs std, so `cargo build` is what holds this honest.
#![cfg_attr(not(test), no_std)]
//! A declarative controller framework.
//!
//! A controller declares what it reacts to and what it does about it as static
//! data. Execution, diagram generation and exhaustive gap checking are all
//! derived from that one declaration, so it is the only place the logic lives.
//!
//! # Layers
//!
//! | Module | For |
//! |---|---|
//! | [`guard`] | The conditions a controller decides by, shared per [`Domain`] |
//! | [`feature`] | One feature of a controller: a rule table over its effects |
//! | [`render`] | Diagrams and tables derived from that declaration |
//! | [`verify`] | Reports over that declaration |
//!
//! There is one kind of table. A controller that keeps state keeps it in its
//! [`Domain::World`] like everything else, where a guard reads it and an action
//! writes it — a named configuration is a field, not a second layer.
//!
//! [`Domain`] holds what a whole controller has in common: its events and its
//! world, and nothing else. Effects are not in common. Each feature names its
//! own action type and carries it out itself ([`feature::Feature::perform`]),
//! so every `perform` is exhaustive over exactly the effects its own file
//! declares, and no file holds effects belonging to another.
//!
//! # Declaration macros
//!
//! | Macro | Generates |
//! |---|---|
//! | [`events!`] | Event enum + kind enum + [`HasKind`] + [`Enumerable`] |
//! | [`cond_node!`] | A [`guard::CondNode`] impl |
//! | [`check!`] | A guard [`guard::Expr`] tree |

extern crate alloc;

mod enums;

pub mod feature;
pub mod guard;
// Builds diagrams and tables for documentation only: no release binary reaches
// it. The examples check their generated document against the committed `.md`
// on every run, so those files are this module's tests.
#[cfg_attr(coverage_nightly, coverage(off))]
pub mod render;
pub mod verify;

#[cfg(test)]
mod tests;

pub use enums::{Enumerable, HasKind};

/// Everything a declaration file names, in one import.
///
/// A table is written against three modules at once — the traits here, the row
/// types in [`feature`], and [`guard`]'s vocabulary — so the imports were the
/// longest part of a short file.
///
/// ```
/// use declared::prelude::*;
///
/// declared::events! { #[derive(Debug)] enum Event => Kind { Toggle } }
///
/// struct Light;
/// impl Domain for Light {
///     type Event = Event;
///     type EventKind = Kind;
///     type World = ();
/// }
/// ```
pub mod prelude {
    pub use crate::feature::{AnyFeature, Feature, Rule};
    pub use crate::guard::{Cond, OnUnknown};
    pub use crate::{Domain, Enumerable, HasKind};
}

use core::fmt::Debug;

/// What one controller's parts have in common: its events and its world. Every
/// other item in this crate is generic over `D: Domain`.
///
/// Actions are deliberately absent. An effect belongs to whoever emits it, so
/// the action vocabulary is named by [`feature::Feature::Action`] instead — one
/// per feature, not one per controller.
pub trait Domain: Sized + 'static {
    /// Event body, including payload. [`events!`] generates this together with
    /// its [`HasKind`] impl.
    type Event: HasKind<Kind = Self::EventKind> + Debug;

    /// Payload-free event tag that rules match on. [`events!`] generates this
    /// together with its [`Enumerable`] impl.
    type EventKind: Enumerable;

    /// The outside world this controller reads and changes (APIs, storage), and
    /// where a controller that keeps state keeps it. `?Sized` so a trait object
    /// can narrow it, e.g. `type World = dyn Foo`.
    type World: ?Sized;

    /// The event kinds the reports walk. Defaults to [`Enumerable::ALL`];
    /// override only to scope them to a subset. It scopes the reports alone —
    /// dispatch still matches every kind.
    fn all_kinds() -> &'static [Self::EventKind] {
        <Self::EventKind as Enumerable>::ALL
    }
}

/// The event type of domain `D`.
pub type EventOf<D> = <D as Domain>::Event;
/// The event kind type of domain `D`.
pub type KindOf<D> = <D as Domain>::EventKind;
/// The world type of domain `D`.
pub type WorldOf<D> = <D as Domain>::World;
