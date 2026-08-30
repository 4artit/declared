//! The state machine layer: a transition table and the executor that runs it.

mod edge;
mod state;

pub use edge::{Edge, Goto, Ignore, Source};
pub use state::State;

use crate::guard::{Cx, Memo};
use crate::{ActionOf, EnvOf, EventOf, HasKind, KindOf, MachineSpec, StateActionOf, verify};

/// The outcome of one [`dispatch`] call, for tests and logs.
///
/// The three lists are borrowed from the tables and each ran in full. Field
/// order is execution order.
pub struct Taken<M: MachineSpec> {
    /// The id of the edge that was selected.
    pub edge: &'static str,
    /// The left state's exit actions. Empty for [`Goto::Internal`].
    pub exit: &'static [StateActionOf<M>],
    /// The selected edge's own actions.
    pub run: &'static [ActionOf<M>],
    /// The entered state's entry actions. Empty for [`Goto::Internal`].
    pub entry: &'static [StateActionOf<M>],
}

// Derives would bound `D` itself; these bound only what is actually used.
impl<M: MachineSpec> std::fmt::Debug for Taken<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Taken")
            .field("edge", &self.edge)
            .field("exit", &self.exit)
            .field("run", &self.run)
            .field("entry", &self.entry)
            .finish()
    }
}

impl<M: MachineSpec> Clone for Taken<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M: MachineSpec> Copy for Taken<M> {}

impl<M: MachineSpec> PartialEq for Taken<M>
where
    ActionOf<M>: PartialEq,
    StateActionOf<M>: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.edge == other.edge
            && self.exit == other.exit
            && self.run == other.run
            && self.entry == other.entry
    }
}

/// The current state of one machine. The table it runs is
/// [`MachineSpec::EDGES`], so this holds nothing but the tag.
pub struct Machine<M: MachineSpec> {
    tag: M::Tag,
}

impl<M: MachineSpec> Machine<M> {
    /// Builds a machine and validates `M`'s tables.
    ///
    /// - `initial`: the state the world is already in, which the caller keeps
    ///   consistent with `Env`. A machine resumes rather than starts, so its
    ///   [`State::entry`] does not run and this call does not touch `Env`. It
    ///   is an argument rather than another const for that reason.
    ///
    /// Returns the constructed machine.
    ///
    /// # Panics
    ///
    /// Panics if `initial`, or any tag [`MachineSpec::all_tags`] lists, or any
    /// edge target, is missing from [`MachineSpec::STATES`]. In debug builds,
    /// also panics if [`verify::coverage`] reports a defect (release builds
    /// skip that check; call [`verify::coverage`] from a test to keep it
    /// enforced there).
    pub fn new(initial: M::Tag) -> Self {
        assert!(
            M::STATES.iter().any(|s| s.tag == initial),
            "initial tag {initial:?} is missing from the state table",
        );
        for &tag in M::all_tags() {
            assert!(
                M::STATES.iter().any(|s| s.tag == tag),
                "tag {tag:?} is listed in MachineSpec::all_tags but not in the state table",
            );
        }
        for e in M::EDGES {
            if let Goto::To(next) = e.goto {
                assert!(
                    M::STATES.iter().any(|s| s.tag == next),
                    "edge {} goes to {next:?}, which is missing from the state table",
                    e.id,
                );
            }
        }

        #[cfg(debug_assertions)]
        {
            let cov = verify::coverage::<M>(initial, M::EDGES, M::IGNORES);
            assert!(cov.is_clean(), "[chart] table has holes: {cov:?}");
        }

        Self { tag: initial }
    }

    /// The state the machine is in. Only [`dispatch`] moves it.
    pub fn tag(&self) -> M::Tag {
        self.tag
    }

    /// Looks up the state table entry for `tag`. [`Machine::new`] guarantees
    /// every tag this is called with is present, so the miss arm is
    /// unreachable.
    fn state_of(tag: M::Tag) -> &'static State<M> {
        M::STATES.iter().find(|s| s.tag == tag).unwrap_or_else(
            // `new` checks every tag against the state table, so this arm
            // cannot be reached.
            #[cfg_attr(coverage_nightly, coverage(off))]
            || unreachable!("no state table entry for {tag:?}"),
        )
    }

    /// Index of the first edge matching `kind` from the current state.
    /// Declaration order is priority.
    fn select(&self, ev: &EventOf<M>, world: &EnvOf<M>, kind: KindOf<M>) -> Option<usize> {
        let memo = Memo::new();
        let cx = Cx::new(ev, world, &memo);

        M::EDGES.iter().position(|e| {
            e.when == kind && e.from.matches(self.tag) && e.unknown.accepts(e.check.eval(&cx))
        })
    }

    /// Runs each of an edge's actions in order.
    fn perform_all(to_run: &[ActionOf<M>], ev: &EventOf<M>, world: &mut EnvOf<M>) {
        for &a in to_run {
            M::perform(a, ev, world);
        }
    }

    /// Runs each of a state's entry or exit actions in order.
    fn perform_state_all(to_run: &[StateActionOf<M>], world: &mut EnvOf<M>) {
        for &a in to_run {
            M::perform_state(a, world);
        }
    }
}

/// Matches `ev` against `M`'s table from `m`'s current state and runs the
/// selected transition.
///
/// - `m`: the machine to advance. Its tag is the only thing that moves.
/// - `ev`: the event to handle.
/// - `world`: the outside world, mutated by whatever actions run.
///
/// Returns the [`Taken`] transition, or `None` if no edge matched (a warning is
/// logged unless the combination is covered by an [`Ignore`]). The return value
/// is for tests and logs; a caller that reads its effects out of `Env` can drop
/// it, which makes the call read like [`crate::feature::AnyFeature::dispatch`].
///
/// Effects run in this order: the current state's [`State::exit`] → the tag
/// changes → `run` → the target state's [`State::entry`]. For
/// [`Goto::Internal`] only `run` executes. Entry and exit go through
/// [`MachineSpec::perform_state`], which is not given the event; only `run`
/// reaches [`MachineSpec::perform`].
///
/// Not re-entrant: `m` is mutably borrowed for the call, so a nested dispatch
/// won't compile. Queue follow-up events in the caller instead — see [`Taken`].
pub fn dispatch<M: MachineSpec>(
    m: &mut Machine<M>,
    ev: &EventOf<M>,
    world: &mut EnvOf<M>,
) -> Option<Taken<M>> {
    let kind = ev.kind();

    let Some(hit) = m.select(ev, world, kind) else {
        if !M::IGNORES.iter().any(|i| i.matches(m.tag, kind)) {
            log::warn!(
                "[chart] unhandled: {:?} x {ev:?} (no edge, no ignore)",
                m.tag
            );
        }
        return None;
    };

    let edge = &M::EDGES[hit];
    let id = edge.id;
    let run = edge.run;

    let target = match edge.goto {
        Goto::To(next) => Some(next),
        Goto::Internal => None,
    };

    // Empty for an internal transition, which stays in its state.
    let mut exit: &'static [StateActionOf<M>] = &[];
    let mut entry: &'static [StateActionOf<M>] = &[];

    if let Some(next) = target {
        exit = Machine::<M>::state_of(m.tag).exit;
        entry = Machine::<M>::state_of(next).entry;
        m.tag = next;
    }

    Machine::<M>::perform_state_all(exit, world);
    Machine::<M>::perform_all(run, ev, world);
    Machine::<M>::perform_state_all(entry, world);

    // `log::debug!` evaluates its arguments only when the level is enabled, so
    // this formats nothing in a release build with logging off.
    log::debug!(
        "[chart] {id}: {ev:?} -> {:?} {exit:?} {run:?} {entry:?}",
        m.tag
    );

    Some(Taken {
        edge: id,
        exit,
        run,
        entry,
    })
}
