//! Mirror folding. `PowerChanged` means different things while folding and
//! while folded, so this one is a state machine.
//!
//! The spec names [`crate::Mirrors`] as its domain, sharing that controller's
//! events, actions, guards and `perform`.

use declared::prelude::*;
use declared::verify::Coverage;
use declared::{render, verify};

use crate::guards::{AtFolded, AtUnfolded, PowerOff, PowerOn, SpeedAllowsFold, SpeedForcesUnfold};
use crate::{Event, Kind, Mirrors, World};

declared::tags! {
    pub enum FoldTag {
        Unfolded,
        Folding,
        Folded,
        Unfolding,
    }
}

/// Effects of this machine being in a state, run whichever edge led there.
/// Declared here rather than on the domain: only a machine has states.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum StateAction {
    Fold,
    Unfold,
}

pub struct FoldSm;

impl MachineSpec for FoldSm {
    const NAME: &'static str = "FoldSm";

    type Domain = Mirrors;
    type Tag = FoldTag;
    type Action = declared::NoAction;
    type StateAction = StateAction;

    const STATES: &'static [State<FoldSm>] = STATES;
    const EDGES: &'static [Edge<FoldSm>] = EDGES;
    const IGNORES: &'static [Ignore<FoldSm>] = IGNORES;

    /// No edge here carries a `run` list: folding is expressed entirely by the
    /// states it passes through, so there is nothing for a transition to do on
    /// its own. `NoAction` says so in the type, and this body cannot be wrong.
    fn perform(action: declared::NoAction, _ev: &Event, _world: &mut World) {
        match action {}
    }

    fn perform_state(action: StateAction, world: &mut World) {
        match action {
            StateAction::Fold => log::debug!("fold (speed {:.0})", world.speed),
            StateAction::Unfold => log::debug!("unfold"),
        }
    }
}

/// The state the mirror is already in when the controller starts. Not a const on
/// the spec: a machine resumes, so this has to stay a runtime choice.
pub const INITIAL: FoldTag = FoldTag::Unfolded;

// ─────────────────────────────────────────── table

/// The motor command is an entry action: it goes out once per arrival, whichever
/// edge led there.
static STATES: &[State<FoldSm>] = &[
    State {
        tag: FoldTag::Unfolded,
        entry: &[],
        exit: &[],
    },
    State {
        tag: FoldTag::Folding,
        entry: &[StateAction::Fold],
        exit: &[],
    },
    State {
        tag: FoldTag::Folded,
        entry: &[],
        exit: &[],
    },
    State {
        tag: FoldTag::Unfolding,
        entry: &[StateAction::Unfold],
        exit: &[],
    },
];

static EDGES: &[Edge<FoldSm>] = &[
    Edge {
        id: "FOLD_START",
        from: Source::These(&[FoldTag::Unfolded]),
        when: Kind::PowerChanged,
        check: declared::check!(PowerOff && SpeedAllowsFold),
        unknown: OnUnknown::Deny,
        emit: &[],
        goto: Goto::To(FoldTag::Folding),
    },
    Edge {
        id: "FOLD_DONE",
        from: Source::These(&[FoldTag::Folding]),
        when: Kind::FoldPositionChanged,
        check: declared::check!(AtFolded),
        unknown: OnUnknown::Deny,
        emit: &[],
        goto: Goto::To(FoldTag::Folded),
    },
    Edge {
        id: "UNFOLD_ON_POWER",
        from: Source::These(&[FoldTag::Folded]),
        when: Kind::PowerChanged,
        check: declared::check!(PowerOn),
        unknown: OnUnknown::Deny,
        emit: &[],
        goto: Goto::To(FoldTag::Unfolding),
    },
    Edge {
        id: "UNFOLD_ON_SPEED",
        from: Source::These(&[FoldTag::Folded]),
        when: Kind::SpeedChanged,
        check: declared::check!(SpeedForcesUnfold),
        unknown: OnUnknown::Deny,
        emit: &[],
        goto: Goto::To(FoldTag::Unfolding),
    },
    Edge {
        id: "UNFOLD_DONE",
        from: Source::These(&[FoldTag::Unfolding]),
        when: Kind::FoldPositionChanged,
        check: declared::check!(AtUnfolded),
        unknown: OnUnknown::Deny,
        emit: &[],
        goto: Goto::To(FoldTag::Unfolded),
    },
];

/// Combinations left alone, with the reason `coverage` needs to tell an omission
/// from a decision.
static IGNORES: &[Ignore<FoldSm>] = &[
    Ignore {
        from: Source::Any,
        when: &[Kind::DefogChanged, Kind::GearChanged, Kind::UserChanged],
        why: "heating, dimming and user switching do not affect folding",
    },
    Ignore {
        from: Source::These(&[FoldTag::Folding, FoldTag::Unfolding]),
        when: &[Kind::PowerChanged],
        why: "a power change must not reverse a motor that is already moving",
    },
    Ignore {
        from: Source::These(&[FoldTag::Unfolded, FoldTag::Folding, FoldTag::Unfolding]),
        when: &[Kind::SpeedChanged],
        why: "the automatic unfold only applies once folded",
    },
    Ignore {
        from: Source::These(&[FoldTag::Unfolded, FoldTag::Folded]),
        when: &[Kind::FoldPositionChanged],
        why: "position reports while stopped have no target to check against",
    },
];

// ─────────────────────────────────────────── machine

/// The controller holds a [`Machine`] directly: with the table on the spec there
/// is nothing left for a wrapper type to carry.
pub fn machine() -> Machine<FoldSm> {
    Machine::new(INITIAL)
}

/// Drawn from the tables alone; no machine instance needed.
pub fn diagram() -> String {
    render::state_diagram::<FoldSm>(INITIAL, EDGES, STATES)
}

/// The kinds this machine acts on, for the controller-wide check.
pub fn handled_kinds() -> Vec<Kind> {
    verify::handled_kinds::<FoldSm>(EDGES)
}

/// The guard nodes this machine names, for the controller-wide check.
pub fn guard_nodes() -> Vec<(&'static str, std::any::TypeId)> {
    verify::guard_nodes::<FoldSm>(EDGES)
}

pub fn coverage() -> Coverage {
    verify::coverage::<FoldSm>(INITIAL, EDGES, IGNORES)
}
