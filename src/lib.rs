#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
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
//! | [`guard`] | The conditions both layers decide by, shared per [`Domain`] |
//! | [`feature`] | Controllers with no states: a rule table per feature |
//! | [`machine`] | Controllers with states: a transition table and its executor |
//! | [`render`] | Diagrams and tables derived from either declaration |
//! | [`verify`] | Exhaustive gap reports over either declaration |
//!
//! [`Domain`] bundles the types a controller works with and is shared by both
//! layers, so a feature that grows states keeps the same declaration. It holds
//! the events and the world — the things a whole controller has in common — and
//! nothing else.
//!
//! Effects are not in common. Each feature and each machine names its own
//! action type and carries it out itself ([`feature::Feature::perform`],
//! [`MachineSpec::perform`], [`MachineSpec::perform_state`]), so every `perform`
//! is exhaustive over exactly the effects its own file declares, and no file
//! holds effects belonging to another.
//!
//! # Declaration macros
//!
//! | Macro | Generates |
//! |---|---|
//! | [`tags!`] | State tag enum + [`Enumerable`] |
//! | [`events!`] | Event enum + kind enum + [`HasKind`] + [`Enumerable`] |
//! | [`cond_node!`] | A [`guard::CondNode`] impl |
//! | [`check!`] | A guard [`guard::Expr`] tree |

mod enums;

pub mod feature;
pub mod guard;
pub mod machine;
// Builds diagrams and tables for documentation only: no release binary reaches
// it, and the committed `.md` files verify its output instead.
#[cfg_attr(coverage_nightly, coverage(off))]
pub mod render;
pub mod verify;

#[cfg(test)]
mod tests;

pub use enums::{Enumerable, HasKind};

use std::fmt::Debug;

/// What one controller's parts have in common: its events and its world. Every
/// other item in this crate is generic over `D: Domain`.
///
/// Actions are deliberately absent. An effect belongs to whoever emits it, so
/// the action vocabulary is named by [`feature::Feature::Action`] and
/// [`MachineSpec::Action`] instead — one per part, not one per controller.
pub trait Domain: Sized + 'static {
    /// Event body, including payload. [`events!`] generates this together with
    /// its [`HasKind`] impl.
    type Event: HasKind<Kind = Self::EventKind> + Debug;

    /// Payload-free event tag that edges match on. [`events!`] generates this
    /// together with its [`Enumerable`] impl.
    type EventKind: Enumerable;

    /// The outside world this controller reads and changes (APIs, storage).
    /// `?Sized` so a trait object can narrow it, e.g. `type Env = dyn Foo`.
    type Env: ?Sized;

    /// The event kinds [`verify::coverage`] walks. Defaults to
    /// [`Enumerable::ALL`]; override only to check a subset. It scopes the
    /// check alone — dispatch still matches every kind.
    fn all_kinds() -> &'static [Self::EventKind] {
        <Self::EventKind as Enumerable>::ALL
    }
}

/// An effect vocabulary with no values, for a part of a controller that
/// produces one kind of effect but not the other.
///
/// A machine whose edges carry no `run` actions names it as
/// [`MachineSpec::Action`], and one with no entry or exit effects names it as
/// [`MachineSpec::StateAction`]. Either way the matching `perform` is
/// `match action {}`: exhaustive over no variants, so it cannot be wrong and
/// cannot be forgotten.
///
/// ```
/// # chart::tags! { pub enum Tag { Only } }
/// # chart::events! { #[derive(Debug)] pub enum Event => Kind { Tick } }
/// # use chart::machine::{Edge, Ignore, Source, State};
/// # pub struct Dom;
/// # impl chart::Domain for Dom {
/// #     type Event = Event;
/// #     type EventKind = Kind;
/// #     type Env = ();
/// # }
/// # pub struct Sm;
/// use chart::NoAction;
///
/// impl chart::MachineSpec for Sm {
/// #   type Domain = Dom;
/// #   type Tag = Tag;
///     type Action = NoAction;
///     type StateAction = NoAction;
/// #   const STATES: &'static [State<Sm>] =
/// #       &[State { tag: Tag::Only, entry: &[], exit: &[] }];
/// #   const EDGES: &'static [Edge<Sm>] = &[];
/// #   const IGNORES: &'static [Ignore<Sm>] =
/// #       &[Ignore { from: Source::Any, when: &[Kind::Tick], why: "nothing to do" }];
///
///     fn perform(action: NoAction, _ev: &Event, _world: &mut ()) {
///         match action {}
///     }
///
///     fn perform_state(action: NoAction, _world: &mut ()) {
///         match action {}
///     }
/// }
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum NoAction {}

/// One state machine's whole declaration: which [`Domain`] it belongs to, what
/// its states are, and the table over them. A domain may name several of these,
/// or none.
///
/// Holding the table here is what [`feature::Feature`] does for the other
/// layer — the declaration lives on the type, so [`machine::dispatch`] needs
/// only an instance. [`verify`] and [`render`] still take tables as arguments,
/// so a check can be run against a table no machine is built from.
pub trait MachineSpec: Sized + 'static {
    /// The vocabulary this machine works in.
    type Domain: Domain;

    /// State identifier. [`tags!`] generates this together with its
    /// [`Enumerable`] impl.
    type Tag: Enumerable;

    /// An effect this machine's edges run: what [`machine::Edge::run`] holds.
    /// [`NoAction`] for a machine whose transitions produce nothing on their
    /// own.
    type Action: Copy + Debug + 'static;

    /// An effect of being in a state: what [`machine::State::entry`] and
    /// [`machine::State::exit`] hold. It belongs to the machine rather than to
    /// the domain, because only a machine has states — a controller with no
    /// machine never names one. Entry and exit run whichever edge led there, so
    /// they cannot read the event; an effect that needs one is a
    /// [`MachineSpec::Action`] on that edge.
    type StateAction: Copy + Debug + 'static;

    /// The states, with their entry and exit effects.
    const STATES: &'static [machine::State<Self>];

    /// The transitions, in priority order: [`machine::dispatch`] takes the
    /// first edge that matches.
    const EDGES: &'static [machine::Edge<Self>];

    /// The combinations deliberately left alone, each with its reason.
    const IGNORES: &'static [machine::Ignore<Self>];

    /// Carries out one action this machine's edges run.
    ///
    /// With [`MachineSpec::perform_state`], the only place this machine may
    /// mutate `Env` — guards only ever see `&Env`.
    ///
    /// - `action`: the effect to carry out.
    /// - `ev`: the event being dispatched, for actions that need a runtime
    ///   value from its payload.
    /// - `world`: the outside world to mutate.
    fn perform(action: ActionOf<Self>, ev: &EventOf<Self>, world: &mut EnvOf<Self>);

    /// Carries out one entry or exit effect. No event: see
    /// [`MachineSpec::StateAction`].
    ///
    /// - `action`: the effect to carry out.
    /// - `world`: the outside world to mutate.
    fn perform_state(action: Self::StateAction, world: &mut EnvOf<Self>);

    /// The states [`verify::coverage`] walks. Defaults to [`Enumerable::ALL`];
    /// override only to check a subset. It scopes the check alone — dispatch
    /// and the diagrams still cover every tag.
    fn all_tags() -> &'static [Self::Tag] {
        <Self::Tag as Enumerable>::ALL
    }
}

/// The event type of `M`'s domain.
pub type EventOf<M> = <<M as MachineSpec>::Domain as Domain>::Event;
/// The event kind type of `M`'s domain.
pub type KindOf<M> = <<M as MachineSpec>::Domain as Domain>::EventKind;
/// The action type of `M`. Like [`StateActionOf`] this is `M`'s own: effects
/// belong to whoever emits them.
pub type ActionOf<M> = <M as MachineSpec>::Action;
/// The state action type of `M`. Unlike the others this is `M`'s own, not its
/// domain's: states belong to the machine.
pub type StateActionOf<M> = <M as MachineSpec>::StateAction;
/// The world type of `M`'s domain.
pub type EnvOf<M> = <<M as MachineSpec>::Domain as Domain>::Env;
