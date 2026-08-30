//! Mirror folding. `PowerChanged` means different things while folding and
//! while folded, so this one is a state machine.
//!
//! The spec names [`crate::Mirrors`] as its domain, sharing that controller's
//! events, actions, guards and `perform`.

use chart::guard::OnUnknown;
use chart::machine::{Edge, Goto, Ignore, Machine, Source, State};
use chart::verify::Coverage;
use chart::{MachineSpec, render, verify};

use crate::guards::{AtFolded, AtUnfolded, PowerOff, PowerOn, SpeedAllowsFold, SpeedForcesUnfold};
use crate::{Kind, Mirrors, StateAction};

chart::tags! {
    pub enum FoldTag {
        Unfolded,
        Folding,
        Folded,
        Unfolding,
    }
}

pub struct FoldSm;

impl MachineSpec for FoldSm {
    type Domain = Mirrors;
    type Tag = FoldTag;

    const STATES: &'static [State<FoldSm>] = STATES;
    const EDGES: &'static [Edge<FoldSm>] = EDGES;
    const IGNORES: &'static [Ignore<FoldSm>] = IGNORES;
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
        check: chart::check!(PowerOff && SpeedAllowsFold),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(FoldTag::Folding),
    },
    Edge {
        id: "FOLD_DONE",
        from: Source::These(&[FoldTag::Folding]),
        when: Kind::FoldPositionChanged,
        check: chart::check!(AtFolded),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(FoldTag::Folded),
    },
    Edge {
        id: "UNFOLD_ON_POWER",
        from: Source::These(&[FoldTag::Folded]),
        when: Kind::PowerChanged,
        check: chart::check!(PowerOn),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(FoldTag::Unfolding),
    },
    Edge {
        id: "UNFOLD_ON_SPEED",
        from: Source::These(&[FoldTag::Folded]),
        when: Kind::SpeedChanged,
        check: chart::check!(SpeedForcesUnfold),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(FoldTag::Unfolding),
    },
    Edge {
        id: "UNFOLD_DONE",
        from: Source::These(&[FoldTag::Unfolding]),
        when: Kind::FoldPositionChanged,
        check: chart::check!(AtUnfolded),
        unknown: OnUnknown::Deny,
        run: &[],
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
    render::to_mermaid::<FoldSm>(INITIAL, EDGES, STATES)
}

/// The kinds this machine acts on, for the controller-wide check.
pub fn handled_kinds() -> Vec<Kind> {
    verify::handled_kinds::<FoldSm>(EDGES)
}

pub fn coverage() -> Coverage {
    verify::coverage::<FoldSm>(INITIAL, EDGES, IGNORES)
}
