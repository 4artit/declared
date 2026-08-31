//! The state machine layer: a transition table and the executor that runs it.

mod edge;
mod state;

pub use edge::{Edge, Goto, Ignore, Source};
pub use state::State;

use alloc::vec::Vec;

use crate::guard::{Cx, Memo};
use crate::{ActionOf, EnvOf, EventOf, HasKind, KindOf, MachineSpec, StateActionOf};

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
    pub emit: &'static [ActionOf<M>],
    /// The entered state's entry actions. Empty for [`Goto::Internal`].
    pub entry: &'static [StateActionOf<M>],
}

// Derives would bound `D` itself; these bound only what is actually used.
impl<M: MachineSpec> core::fmt::Debug for Taken<M> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Taken")
            .field("edge", &self.edge)
            .field("exit", &self.exit)
            .field("emit", &self.emit)
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
            && self.emit == other.emit
            && self.entry == other.entry
    }
}

/// What matching an event against the table produced. Private: it exists to
/// keep `dispatch`'s two "nothing happened" cases apart, which callers see as
/// the difference between a `warn` and a `debug` line.
enum Selected {
    /// Take the edge at this index.
    Take(usize),
    /// Rows cover this combination, but every guard declined. Ordinary.
    Declined,
    /// No row covers this combination at all.
    NoRow,
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
    /// also panics if [`crate::verify::coverage`] reports a defect (release builds
    /// skip that check; call [`crate::verify::coverage`] from a test to keep it
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
            let cov = crate::verify::coverage::<M>(initial, M::EDGES, M::IGNORES);
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

    /// Matches `kind` against the table from the current state. Declaration
    /// order is priority.
    ///
    /// Separating [`Selected::Declined`] from [`Selected::NoRow`] is what lets
    /// `dispatch` tell a gap in the table from a guard doing its job: the first
    /// is a defect, the second is the ordinary way a guarded row is skipped.
    fn select(
        &self,
        ev: &EventOf<M::Domain>,
        world: &EnvOf<M::Domain>,
        kind: KindOf<M::Domain>,
    ) -> Selected {
        let memo = Memo::new();
        let cx = Cx::new(ev, world, &memo);

        // One `Memo` across the whole scan, so a node shared by several rows is
        // evaluated once per dispatch.
        let mut saw_row = false;
        for (i, e) in M::EDGES.iter().enumerate() {
            if e.when != kind || !e.from.matches(self.tag) {
                continue;
            }
            saw_row = true;
            if e.unknown.accepts(e.check.eval(&cx)) {
                return Selected::Take(i);
            }
        }

        if saw_row {
            Selected::Declined
        } else {
            Selected::NoRow
        }
    }

    /// The ids of the rows covering `kind` from the current state, guards not
    /// consulted. Only for the [`Selected::Declined`] log line, which builds it
    /// solely when debug logging is on.
    fn rows_for(&self, kind: KindOf<M::Domain>) -> Vec<&'static str> {
        M::EDGES
            .iter()
            .filter(|e| e.when == kind && e.from.matches(self.tag))
            .map(|e| e.id)
            .collect()
    }

    /// Runs each of an edge's actions in order.
    fn perform_all(to_run: &[ActionOf<M>], ev: &EventOf<M::Domain>, world: &mut EnvOf<M::Domain>) {
        for &a in to_run {
            M::perform(a, ev, world);
        }
    }

    /// Runs each of a state's entry or exit actions in order.
    fn perform_state_all(to_run: &[StateActionOf<M>], world: &mut EnvOf<M::Domain>) {
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
    ev: &EventOf<M::Domain>,
    world: &mut EnvOf<M::Domain>,
) -> Option<Taken<M>> {
    let kind = ev.kind();

    let hit = match m.select(ev, world, kind) {
        Selected::Take(hit) => hit,
        // Every row said no. That is what a guard is for, so it is not a
        // defect — but it is the usual reason an event "did nothing", so it is
        // worth a line when tracing.
        Selected::Declined => {
            log::debug!(
                "[chart] {}/declined: {:?} x {ev:?} (every guard said no: {:?})",
                M::NAME,
                m.tag,
                m.rows_for(kind)
            );
            return None;
        }
        // A combination the table does not cover. `verify::coverage` reports
        // this statically, so reaching it means the check never ran over this
        // combination — a release build, or a narrowed `all_tags`/`all_kinds`.
        Selected::NoRow => {
            if !M::IGNORES.iter().any(|i| i.matches(m.tag, kind)) {
                log::warn!(
                    "[chart] {}/no row: {:?} x {ev:?} (no edge, no ignore)",
                    M::NAME,
                    m.tag
                );
            }
            return None;
        }
    };

    let edge = &M::EDGES[hit];
    let id = edge.id;
    let emit = edge.emit;

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
    Machine::<M>::perform_all(emit, ev, world);
    Machine::<M>::perform_state_all(entry, world);

    // `log::debug!` evaluates its arguments only when the level is enabled, so
    // this formats nothing in a release build with logging off.
    log::debug!(
        "[chart] {}/{id}: {ev:?} -> {:?} {exit:?} {emit:?} {entry:?}",
        M::NAME,
        m.tag
    );

    Some(Taken {
        edge: id,
        exit,
        emit,
        entry,
    })
}
