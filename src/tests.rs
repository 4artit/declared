//! Tests for the framework itself.
//!
//! Uses a reverse-camera controller, slightly richer than the README quick-start
//! example, to exercise every feature.

// The fixture's event names read better with the shared `Changed` suffix, and
// `events!` emits the same variant names for both enums.
#![allow(clippy::enum_variant_names)]

use std::cell::Cell;

use super::feature::{self, Feature, FeatureInfo, Rule};
use super::guard::{Cond, Cx, Expr, Memo, OnUnknown};
use super::machine::{self, Edge, Goto, Ignore, Machine, Source, State, Taken};
use super::{Domain, Enumerable, HasKind, MachineSpec, render, verify};

// ─────────────────────────────────────────── domain

crate::tags! {
    enum Tag {
        Off,
        Showing,
    }
}

crate::events! {
    #[derive(Clone, Debug)]
    enum Event => Kind {
        GearChanged(Gear),
        SpeedChanged,
        PowerChanged,
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Gear {
    Reverse,
    Drive,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Action {
    ShowCamera,
    HideCamera,
    UpdateOverlay,
}

// Only needed by `feature::unemitted_actions`; `Domain` does not require it.
impl Enumerable for Action {
    const ALL: &'static [Self] = &[Self::ShowCamera, Self::HideCamera, Self::UpdateOverlay];
}

#[derive(Default)]
struct Env {
    /// `None` models a failed lookup, which yields `Cond::Unknown`.
    speed: Option<f32>,
    camera_visible: bool,
    performed: Vec<Action>,
    /// The event kind each action was performed for, recorded from `perform`'s
    /// `ev` argument. Entry and exit add nothing: `perform_state` has no event.
    performed_for: Vec<Kind>,
    /// How many times `SpeedBelowLimit` looked the speed up. Guards receive
    /// `&Env`, so this needs interior mutability.
    speed_lookups: Cell<u32>,
}

struct RearCam;

impl Domain for RearCam {
    type Event = Event;
    type EventKind = Kind;
    type Action = Action;
    // The feature fixtures below emit the same actions this machine runs on
    // entry and exit, so the two vocabularies are aliased onto one enum.
    type StateAction = Action;
    type Env = Env;

    fn perform(action: Action, ev: &Event, world: &mut Env) {
        world.performed_for.push(ev.kind());
        Self::perform_state(action, world);
    }

    fn perform_state(action: Action, world: &mut Env) {
        world.performed.push(action);
        match action {
            Action::ShowCamera => world.camera_visible = true,
            Action::HideCamera => world.camera_visible = false,
            Action::UpdateOverlay => {}
        }
    }
}

impl MachineSpec for RearCam {
    type Domain = RearCam;
    type Tag = Tag;

    const STATES: &'static [State<RearCam>] = STATES;
    const EDGES: &'static [Edge<RearCam>] = EDGES;
    const IGNORES: &'static [Ignore<RearCam>] = IGNORES;
}

// ─────────────────────────────────────────── guards
// A payload guard: pure, and can never be Unknown.

crate::cond_node!(RearCam, GearIsReverse, |cx| match cx.event {
    Event::GearChanged(g) => Cond::from(*g == Gear::Reverse),
    _ => Cond::False,
});

// A context guard: Unknown when the lookup fails. Counts its lookups so that
// short-circuiting is observable.
crate::cond_node!(RearCam, SpeedBelowLimit, |cx| {
    cx.world.speed_lookups.set(cx.world.speed_lookups.get() + 1);
    match cx.world.speed {
        Some(v) => Cond::from(v < 15.0),
        None => Cond::Unknown,
    }
});

// ─────────────────────────────────────────── states

static STATES: &[State<RearCam>] = &[
    State {
        tag: Tag::Off,
        entry: &[],
        exit: &[],
    },
    State {
        tag: Tag::Showing,
        entry: &[Action::ShowCamera],
        exit: &[Action::HideCamera],
    },
];

// ─────────────────────────────────────────── table

static EDGES: &[Edge<RearCam>] = &[
    Edge {
        id: "CAM_ON",
        from: Source::These(&[Tag::Off]),
        when: Kind::GearChanged,
        check: crate::check!(GearIsReverse && SpeedBelowLimit),
        unknown: OnUnknown::Deny, // unknown speed: do not turn it on
        run: &[],
        goto: Goto::To(Tag::Showing),
    },
    Edge {
        id: "CAM_OFF_GEAR",
        from: Source::These(&[Tag::Showing]),
        when: Kind::GearChanged,
        check: crate::check!(!GearIsReverse),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Off),
    },
    Edge {
        id: "CAM_OFF_SPEED",
        from: Source::These(&[Tag::Showing]),
        when: Kind::SpeedChanged,
        check: crate::check!(!SpeedBelowLimit),
        unknown: OnUnknown::Allow, // unknown speed: turn it off
        run: &[],
        goto: Goto::To(Tag::Off),
    },
    Edge {
        id: "CAM_OVERLAY",
        from: Source::These(&[Tag::Showing]),
        when: Kind::SpeedChanged,
        check: crate::check!(SpeedBelowLimit),
        unknown: OnUnknown::Deny,
        run: &[Action::UpdateOverlay],
        goto: Goto::Internal, // stays in state, so exit/enter do not run
    },
];

static IGNORES: &[Ignore<RearCam>] = &[
    Ignore {
        from: Source::These(&[Tag::Off]),
        when: &[Kind::SpeedChanged],
        why: "speed is irrelevant while the camera is not shown",
    },
    Ignore {
        from: Source::Any,
        when: &[Kind::PowerChanged],
        why: "power is handled by the parent controller",
    },
];

fn off() -> Machine<RearCam> {
    Machine::new(Tag::Off)
}

fn showing() -> (Machine<RearCam>, Env) {
    let mut m = off();
    let mut w = Env {
        speed: Some(10.0),
        ..Default::default()
    };
    machine::dispatch(&mut m, &Event::GearChanged(Gear::Reverse), &mut w);
    assert_eq!(m.tag(), Tag::Showing);
    w.performed.clear();
    w.performed_for.clear();
    (m, w)
}

// ─────────────────────────────────────────── tests

#[test]
fn enters_showing_and_runs_entry_action() {
    let mut m = off();
    let mut w = Env {
        speed: Some(10.0),
        ..Default::default()
    };

    let taken = machine::dispatch(&mut m, &Event::GearChanged(Gear::Reverse), &mut w).unwrap();

    assert_eq!(taken.edge, "CAM_ON");
    assert_eq!(taken.entry, [Action::ShowCamera]);
    assert!(taken.exit.is_empty());
    assert!(taken.run.is_empty());
    assert_eq!(m.tag(), Tag::Showing);
    assert_eq!(w.performed, vec![Action::ShowCamera]);
    assert!(w.camera_visible);
}

/// `initial` names the state the world is already in, so its entry belongs to a
/// past run and is not replayed. Its exit still runs on the way out.
#[test]
fn the_initial_state_is_resumed_not_entered() {
    // The world arrives with the camera already on.
    let mut w = Env {
        speed: Some(10.0),
        camera_visible: true,
        ..Default::default()
    };

    let mut m = Machine::<RearCam>::new(Tag::Showing);
    assert!(w.performed.is_empty(), "ShowCamera must not be replayed");

    machine::dispatch(&mut m, &Event::GearChanged(Gear::Drive), &mut w).unwrap();

    assert_eq!(w.performed, vec![Action::HideCamera]);
    assert!(!w.camera_visible);
}

/// `Taken`'s impls are hand-written so that they bound the action types rather
/// than `D`, which a derive would have required.
#[test]
fn taken_is_debug_clone_and_eq() {
    let mut m = off();
    let mut w = Env {
        speed: Some(10.0),
        ..Default::default()
    };

    let on = machine::dispatch(&mut m, &Event::GearChanged(Gear::Reverse), &mut w).unwrap();

    assert_eq!(
        format!("{on:?}"),
        r#"Taken { edge: "CAM_ON", exit: [], run: [], entry: [ShowCamera] }"#
    );
    assert_eq!(on.clone(), on);

    let off = machine::dispatch(&mut m, &Event::GearChanged(Gear::Drive), &mut w).unwrap();
    assert_ne!(off, on);
}

/// Two transitions can share an edge id only by mistake, so `eq` compares the
/// action lists as well. Values differing only after `edge` prove it reads them.
#[test]
fn taken_eq_compares_every_field() {
    let base: Taken<RearCam> = Taken {
        edge: "CAM_ON",
        exit: &[],
        run: &[],
        entry: &[],
    };

    assert_eq!(base, base);
    assert_ne!(
        base,
        Taken {
            exit: &[Action::HideCamera],
            ..base
        }
    );
    assert_ne!(
        base,
        Taken {
            run: &[Action::UpdateOverlay],
            ..base
        }
    );
    assert_ne!(
        base,
        Taken {
            entry: &[Action::ShowCamera],
            ..base
        }
    );
}

#[test]
fn exit_action_runs_on_leaving() {
    let (mut m, mut w) = showing();

    machine::dispatch(&mut m, &Event::GearChanged(Gear::Drive), &mut w);

    assert_eq!(m.tag(), Tag::Off);
    assert_eq!(w.performed, vec![Action::HideCamera]);
    assert!(!w.camera_visible);
}

#[test]
fn exit_and_entry_actions_run_in_order_across_a_round_trip() {
    let mut m = off();
    let mut w = Env {
        speed: Some(10.0),
        ..Default::default()
    };

    machine::dispatch(&mut m, &Event::GearChanged(Gear::Reverse), &mut w);
    machine::dispatch(&mut m, &Event::GearChanged(Gear::Drive), &mut w);

    assert_eq!(m.tag(), Tag::Off);
    assert_eq!(w.performed, vec![Action::ShowCamera, Action::HideCamera]);
    assert!(!w.camera_visible);
}

#[test]
fn internal_transition_skips_exit_and_entry() {
    let (mut m, mut w) = showing();

    let taken = machine::dispatch(&mut m, &Event::SpeedChanged, &mut w).unwrap();

    assert_eq!(taken.edge, "CAM_OVERLAY");
    assert_eq!(m.tag(), Tag::Showing);
    // Neither HideCamera nor ShowCamera slips in.
    assert_eq!(w.performed, vec![Action::UpdateOverlay]);
}

#[test]
fn unknown_denies_transition_when_policy_is_deny() {
    let mut m = off();
    let mut w = Env {
        speed: None, // failed lookup -> SpeedBelowLimit = Unknown
        ..Default::default()
    };

    assert!(machine::dispatch(&mut m, &Event::GearChanged(Gear::Reverse), &mut w).is_none());
    assert_eq!(m.tag(), Tag::Off);
    assert!(w.performed.is_empty());
}

#[test]
fn unknown_allows_transition_when_policy_is_allow() {
    let (mut m, mut w) = showing();

    w.speed = None; // !SpeedBelowLimit = Unknown, and the policy is Allow
    let taken = machine::dispatch(&mut m, &Event::SpeedChanged, &mut w).unwrap();

    assert_eq!(taken.edge, "CAM_OFF_SPEED");
    assert_eq!(m.tag(), Tag::Off);
}

#[test]
fn declaration_order_is_priority() {
    // Both CAM_OFF_SPEED and CAM_OVERLAY match (Showing, SpeedChanged). Their
    // guards are mutually exclusive, so there is no real conflict; this pins down
    // that declaration order decides.
    let (mut m, mut w) = showing();

    w.speed = Some(20.0); // !SpeedBelowLimit = True -> the earlier CAM_OFF_SPEED
    assert_eq!(
        machine::dispatch(&mut m, &Event::SpeedChanged, &mut w)
            .unwrap()
            .edge,
        "CAM_OFF_SPEED"
    );
}

#[test]
fn declared_ignore_is_not_a_hole() {
    let mut m = off();
    let mut w = Env::default();

    assert!(machine::dispatch(&mut m, &Event::PowerChanged, &mut w).is_none());
    assert_eq!(m.tag(), Tag::Off);
}

#[test]
fn coverage_has_no_holes_and_no_unreachable_state() {
    let c = verify::coverage::<RearCam>(Tag::Off, EDGES, IGNORES);

    assert!(c.holes.is_empty(), "holes: {:?}", c.holes);
    assert!(c.unreachable.is_empty(), "unreachable: {:?}", c.unreachable);
    assert!(c.is_clean());

    // Overlaps are reported even when the guards are mutually exclusive.
    assert_eq!(c.overlaps.len(), 1);
    assert_eq!(c.overlaps[0].2, vec!["CAM_OFF_SPEED", "CAM_OVERLAY"]);
}

#[test]
fn mermaid_matches_golden() {
    let expected = "\
stateDiagram-v2
    [*] --> Off
    Showing : Showing<br/>entry / ShowCamera<br/>exit / HideCamera
    Off --> Showing: GearChanged<br/>[GearIsReverse && SpeedBelowLimit]
    Showing --> Off: GearChanged<br/>[!GearIsReverse]
    Showing --> Off: SpeedChanged<br/>[!SpeedBelowLimit]<br/>unknown=Allow
";

    // No machine needed: the diagram comes from the static tables alone.
    assert_eq!(
        render::to_mermaid::<RearCam>(Tag::Off, EDGES, STATES),
        expected
    );
}

#[test]
fn internal_table_lists_state_preserving_edges() {
    let table = render::internal_table::<RearCam>(EDGES);

    assert!(table.contains("CAM_OVERLAY"), "{table}");
    assert!(table.contains("UpdateOverlay"), "{table}");
    // Edges that change state are not in this table.
    assert!(!table.contains("CAM_ON"), "{table}");
}

#[test]
fn caller_side_queue_processes_events_in_order() {
    use std::collections::VecDeque;

    let mut m = off();
    let mut w = Env {
        speed: Some(10.0),
        ..Default::default()
    };

    // Machine owns no queue. Driving it from the caller exposes each Taken.
    let mut pending = VecDeque::from([Event::GearChanged(Gear::Reverse), Event::SpeedChanged]);
    let mut taken = Vec::new();
    while let Some(ev) = pending.pop_front() {
        if let Some(t) = machine::dispatch(&mut m, &ev, &mut w) {
            taken.push(t.edge);
        }
    }

    assert_eq!(taken, vec!["CAM_ON", "CAM_OVERLAY"]);
    assert_eq!(m.tag(), Tag::Showing);
    assert_eq!(w.performed, vec![Action::ShowCamera, Action::UpdateOverlay]);
}

#[test]
fn perform_sees_the_event_on_an_internal_transition() {
    let (mut m, mut w) = showing();

    // CAM_OVERLAY is Goto::Internal, so neither entry nor exit runs. Its action
    // can still read the event, which is the only route to a payload here
    // because Edge::run holds compile-time constants only.
    let taken = machine::dispatch(&mut m, &Event::SpeedChanged, &mut w).unwrap();

    assert_eq!(taken.edge, "CAM_OVERLAY");
    assert_eq!(w.performed, vec![Action::UpdateOverlay]);
    assert_eq!(w.performed_for, vec![Kind::SpeedChanged]);
}

/// The counterpart of the test above: entry and exit go through `perform_state`,
/// which is handed no event, so nothing lands in `performed_for`.
#[test]
fn entry_and_exit_actions_are_performed_without_an_event() {
    let (mut m, mut w) = showing();

    machine::dispatch(&mut m, &Event::GearChanged(Gear::Drive), &mut w);

    assert_eq!(w.performed, vec![Action::HideCamera]);
    assert!(w.performed_for.is_empty());
}

#[test]
fn ignore_table_lists_reasons() {
    let table = render::ignore_table::<RearCam>(IGNORES);

    assert!(table.contains("speed is irrelevant"), "{table}");
    assert!(table.contains("power is handled"), "{table}");
    // Source::Any is expanded into concrete states.
    assert!(table.contains("`Off` | `PowerChanged`"), "{table}");
    assert!(table.contains("`Showing` | `PowerChanged`"), "{table}");
}

#[test]
fn render_parenthesises_negated_subexpressions() {
    // check! only negates single nodes, but Expr can be built by hand (the
    // documented route for `||`), and then precedence must survive rendering.
    static NEGATED_AND: Expr<RearCam> = Expr::Not(&Expr::And(
        &Expr::Node(&GearIsReverse),
        &Expr::Node(&SpeedBelowLimit),
    ));

    assert_eq!(NEGATED_AND.render(), "!(GearIsReverse && SpeedBelowLimit)");
    // A negated single node needs no parentheses.
    assert_eq!(crate::check!(!GearIsReverse).render(), "!GearIsReverse");
}

/// `check!()` with no arguments. An edge with no guard is always taken.
#[test]
fn always_is_true_renders_empty_and_references_no_nodes() {
    let w = Env::default();
    let ev = Event::PowerChanged;
    let memo = Memo::new();
    let cx: Cx<'_, RearCam> = Cx::new(&ev, &w, &memo);

    let always: &Expr<RearCam> = crate::check!();

    assert_eq!(always.eval(&cx), Cond::True);
    assert_eq!(always.render(), "");

    let mut ids = Vec::new();
    always.node_ids(&mut ids);
    assert!(ids.is_empty());
}

/// `||` has no macro form, so `Expr::Or` is built by hand.
#[test]
fn or_short_circuits_on_true() {
    static EITHER: Expr<RearCam> =
        Expr::Or(&Expr::Node(&GearIsReverse), &Expr::Node(&SpeedBelowLimit));

    assert_eq!(EITHER.render(), "(GearIsReverse || SpeedBelowLimit)");

    let mut ids = Vec::new();
    EITHER.node_ids(&mut ids);
    assert_eq!(ids.len(), 2);

    // Left is True, so the speed is never looked up.
    let w = Env {
        speed: None,
        ..Default::default()
    };
    let ev = Event::GearChanged(Gear::Reverse);
    let memo = Memo::new();
    assert_eq!(EITHER.eval(&Cx::new(&ev, &w, &memo)), Cond::True);
    assert_eq!(w.speed_lookups.get(), 0);

    // Left is False, so the right operand decides — and its lookup fails.
    let ev = Event::GearChanged(Gear::Drive);
    let memo = Memo::new();
    assert_eq!(EITHER.eval(&Cx::new(&ev, &w, &memo)), Cond::Unknown);
    assert_eq!(w.speed_lookups.get(), 1);
}

/// A `False` on the left settles an `And`, so the right operand is skipped.
#[test]
fn and_short_circuits_on_false() {
    let w = Env {
        speed: None,
        ..Default::default()
    };
    let ev = Event::GearChanged(Gear::Drive); // GearIsReverse -> False
    let memo = Memo::new();

    let both = crate::check!(GearIsReverse && SpeedBelowLimit);

    assert_eq!(both.eval(&Cx::new(&ev, &w, &memo)), Cond::False);
    assert_eq!(w.speed_lookups.get(), 0);
}

#[test]
fn macros_generate_exhaustive_lists() {
    use crate::Enumerable;

    // The macros supply what all_tags/all_kinds used to spell out by hand.
    assert_eq!(RearCam::all_tags(), &[Tag::Off, Tag::Showing]);
    assert_eq!(
        RearCam::all_kinds(),
        &[Kind::GearChanged, Kind::SpeedChanged, Kind::PowerChanged]
    );
    assert_eq!(Tag::ALL.len(), 2);
    assert_eq!(Kind::ALL.len(), 3);
}

#[test]
fn generated_kind_maps_payload_and_unit_variants() {
    use crate::HasKind;

    // Payload-carrying and unit variants expand under the same rule.
    assert_eq!(Event::GearChanged(Gear::Reverse).kind(), Kind::GearChanged);
    assert_eq!(Event::GearChanged(Gear::Drive).kind(), Kind::GearChanged);
    assert_eq!(Event::SpeedChanged.kind(), Kind::SpeedChanged);
    assert_eq!(Event::PowerChanged.kind(), Kind::PowerChanged);
}

#[test]
fn source_any_except_matches_all_but_listed() {
    let s = Source::<RearCam>::AnyExcept(&[Tag::Showing]);

    assert!(s.matches(Tag::Off));
    assert!(!s.matches(Tag::Showing));
}

#[test]
fn source_any_matches_every_tag() {
    let s = Source::<RearCam>::Any;

    assert!(s.matches(Tag::Off));
    assert!(s.matches(Tag::Showing));
}

#[test]
fn ignore_any_except_matches_multiple_tags() {
    let ignore = Ignore::<RearCam> {
        from: Source::AnyExcept(&[Tag::Showing]),
        when: &[Kind::PowerChanged],
        why: "test",
    };

    assert!(ignore.matches(Tag::Off, Kind::PowerChanged));
    assert!(!ignore.matches(Tag::Showing, Kind::PowerChanged));
    assert!(!ignore.matches(Tag::Off, Kind::GearChanged));
}

// ─────────────────────────────────────────── narrowed domain
// `Domain::all_tags` may be overridden to check a subset, which takes the excluded
// tags out of the first validation loop. Edge targets are checked separately.

struct PartialCam;

impl Domain for PartialCam {
    type Event = Event;
    type EventKind = Kind;
    type Action = Action;
    type StateAction = Action;
    type Env = Env;

    fn perform(_action: Action, _ev: &Event, _world: &mut Env) {}
    fn perform_state(_action: Action, _world: &mut Env) {}
}

impl MachineSpec for PartialCam {
    type Domain = PartialCam;
    type Tag = Tag;

    const STATES: &'static [State<PartialCam>] = PARTIAL_STATES;
    const EDGES: &'static [Edge<PartialCam>] = PARTIAL_EDGES;
    const IGNORES: &'static [Ignore<PartialCam>] = PARTIAL_IGNORES;

    fn all_tags() -> &'static [Tag] {
        &[Tag::Off]
    }
}

static PARTIAL_STATES: &[State<PartialCam>] = &[State {
    tag: Tag::Off,
    entry: &[],
    exit: &[],
}];

static PARTIAL_EDGES: &[Edge<PartialCam>] = &[Edge {
    id: "TO_UNDECLARED",
    from: Source::These(&[Tag::Off]),
    when: Kind::GearChanged,
    check: crate::check!(),
    unknown: OnUnknown::Deny,
    run: &[],
    goto: Goto::To(Tag::Showing), // absent from all_tags and from PARTIAL_STATES
}];

static PARTIAL_IGNORES: &[Ignore<PartialCam>] = &[Ignore {
    from: Source::Any,
    when: &[Kind::SpeedChanged, Kind::PowerChanged],
    why: "outside this fixture",
}];

/// A `Goto::To` pointing outside the state table is rejected at construction, not
/// when the transition is eventually taken.
#[test]
#[should_panic(expected = "edge TO_UNDECLARED goes to Showing")]
fn an_edge_targeting_a_tag_outside_the_state_table_is_rejected() {
    let _ = Machine::<PartialCam>::new(Tag::Off);
}

/// Narrowing `all_tags` scopes the walk: only `Off` is checked, so `Showing`
/// shows up as neither a hole nor unreachable.
#[test]
fn a_narrowed_spec_checks_only_the_listed_tags() {
    let c = verify::coverage::<PartialCam>(Tag::Off, PARTIAL_EDGES, PARTIAL_IGNORES);

    assert!(c.holes.is_empty(), "{:?}", c.holes);
    assert!(c.unreachable.is_empty(), "{:?}", c.unreachable);
    assert!(c.is_clean());
}

/// `all_tags` narrows the coverage check, nothing else. `expand` walks every tag
/// so that diagrams keep matching what `matches` does at dispatch.
#[test]
fn expand_covers_every_tag_even_where_all_tags_is_narrowed() {
    assert_eq!(PartialCam::all_tags(), &[Tag::Off]);

    let any: Source<PartialCam> = Source::Any;
    assert_eq!(any.expand(), vec![Tag::Off, Tag::Showing]);
    assert!(any.matches(Tag::Showing));
}

// ─────────────────────────────────────────── every defect at once
// `coverage` is generic, so each spec it is used with is compiled separately.
// Feeding a table carrying every defect class through the main fixture keeps
// that copy exercised end to end.

crate::cond_node!(RearCam, Duplicated, |_cx| Cond::True);

struct AlsoDuplicated;
struct StillDuplicated;

impl crate::guard::CondNode<RearCam> for AlsoDuplicated {
    fn name(&self) -> &'static str {
        "Duplicated"
    }
    fn eval(&self, _cx: &Cx<'_, RearCam>) -> Cond {
        Cond::True
    }
}

// A third type on the same name, so the report is proved to list it once.
impl crate::guard::CondNode<RearCam> for StillDuplicated {
    fn name(&self) -> &'static str {
        "Duplicated"
    }
    fn eval(&self, _cx: &Cx<'_, RearCam>) -> Cond {
        Cond::True
    }
}

static DEFECTIVE_EDGES: &[Edge<RearCam>] = &[
    Edge {
        id: "DUPED",
        from: Source::These(&[Tag::Off]),
        when: Kind::GearChanged,
        check: crate::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Off),
    },
    Edge {
        id: "DUPED", // same id, and a second edge on the same combination
        from: Source::These(&[Tag::Off]),
        when: Kind::GearChanged,
        check: crate::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::Internal,
    },
    Edge {
        id: "DUPED", // a third, to be reported once
        from: Source::These(&[Tag::Off]),
        when: Kind::SpeedChanged,
        check: &Expr::And(
            &Expr::Node(&Duplicated),
            &Expr::And(&Expr::Node(&AlsoDuplicated), &Expr::Node(&StillDuplicated)),
        ),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Off),
    },
    Edge {
        id: "STUCK", // leaves and enters a state nothing else reaches
        from: Source::These(&[Tag::Showing]),
        when: Kind::PowerChanged,
        check: crate::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Showing),
    },
];

static DEFECTIVE_IGNORES: &[Ignore<RearCam>] = &[Ignore {
    from: Source::These(&[Tag::Off]),
    when: &[Kind::GearChanged],
    why: "test: contradicted by two edges",
}];

#[test]
fn coverage_reports_every_defect_class_from_one_table() {
    let c = verify::coverage::<RearCam>(Tag::Off, DEFECTIVE_EDGES, DEFECTIVE_IGNORES);

    assert!(!c.is_clean());
    assert_eq!(c.duplicate_edge_ids, vec!["DUPED"]);
    assert_eq!(c.duplicate_node_names, vec!["Duplicated"]);
    assert_eq!(c.unreachable, vec!["Showing"]);
    assert_eq!(
        c.ignored_but_handled,
        vec![(
            "Off".to_owned(),
            "GearChanged".to_owned(),
            vec!["DUPED", "DUPED"]
        )]
    );
    assert!(
        c.holes
            .contains(&("Off".to_owned(), "PowerChanged".to_owned())),
        "{:?}",
        c.holes
    );
    assert_eq!(c.overlaps.len(), 1, "{:?}", c.overlaps);
}

// ─────────────────────────────────────────── reachability chain
// Reachability iterates to a fixed point, so a chain whose edges are declared
// out of order takes more than one pass over the table.

crate::tags! {
    enum ChainTag {
        First,
        Middle,
        Last,
    }
}

struct ChainSm;

impl MachineSpec for ChainSm {
    type Domain = RearCam;
    type Tag = ChainTag;

    const STATES: &'static [State<ChainSm>] = CHAIN_STATES;
    const EDGES: &'static [Edge<ChainSm>] = CHAIN_EDGES;
    const IGNORES: &'static [Ignore<ChainSm>] = &[];
}

static CHAIN_STATES: &[State<ChainSm>] = &[
    State {
        tag: ChainTag::First,
        entry: &[],
        exit: &[],
    },
    State {
        tag: ChainTag::Middle,
        entry: &[],
        exit: &[],
    },
    State {
        tag: ChainTag::Last,
        entry: &[],
        exit: &[],
    },
];

static CHAIN_EDGES: &[Edge<ChainSm>] = &[
    // Declared before the edge that makes `Middle` reachable at all.
    Edge {
        id: "MIDDLE_TO_LAST",
        from: Source::These(&[ChainTag::Middle]),
        when: Kind::SpeedChanged,
        check: crate::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(ChainTag::Last),
    },
    Edge {
        id: "FIRST_TO_MIDDLE",
        from: Source::These(&[ChainTag::First]),
        when: Kind::GearChanged,
        check: crate::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(ChainTag::Middle),
    },
];

#[test]
fn reachability_follows_a_chain_declared_out_of_order() {
    let c = verify::coverage::<ChainSm>(ChainTag::First, CHAIN_EDGES, &[]);

    assert!(c.unreachable.is_empty(), "{:?}", c.unreachable);
}

// ─────────────────────────────────────────── defective table
// RearCam is deliberately clean, so the diagnostics never fire on it. This table
// trips each of them: a hole, an unreachable state, one guard name shared by
// three node types, one id shared by two edges, and an `Ignore` an edge
// contradicts.

struct Broken;

impl Domain for Broken {
    type Event = Event;
    type EventKind = Kind;
    type Action = Action;
    type StateAction = Action;
    type Env = Env;

    fn perform(_action: Action, _ev: &Event, _world: &mut Env) {}
    fn perform_state(_action: Action, _world: &mut Env) {}
}

impl MachineSpec for Broken {
    type Domain = Broken;
    type Tag = Tag;

    const STATES: &'static [State<Broken>] = BROKEN_STATES;
    const EDGES: &'static [Edge<Broken>] = BROKEN_EDGES;
    const IGNORES: &'static [Ignore<Broken>] = BROKEN_IGNORES;
}

crate::cond_node!(Broken, Duplicate, |_cx| Cond::True);

struct AlsoDuplicate;
struct StillDuplicate;

impl crate::guard::CondNode<Broken> for AlsoDuplicate {
    fn name(&self) -> &'static str {
        "Duplicate"
    }
    fn eval(&self, _cx: &Cx<'_, Broken>) -> Cond {
        Cond::True
    }
}

impl crate::guard::CondNode<Broken> for StillDuplicate {
    fn name(&self) -> &'static str {
        "Duplicate"
    }
    fn eval(&self, _cx: &Cx<'_, Broken>) -> Cond {
        Cond::True
    }
}

/// `Showing` is missing, so `to_mermaid` has no description to draw for it.
static BROKEN_STATES: &[State<Broken>] = &[State {
    tag: Tag::Off,
    entry: &[],
    exit: &[],
}];

static BROKEN_EDGES: &[Edge<Broken>] = &[
    Edge {
        id: "NO_GUARD",
        from: Source::These(&[Tag::Off]),
        when: Kind::GearChanged,
        check: crate::check!(),
        unknown: OnUnknown::Deny,
        run: &[Action::UpdateOverlay],
        goto: Goto::To(Tag::Off), // nothing reaches Showing
    },
    Edge {
        id: "DUPED_NAMES",
        from: Source::These(&[Tag::Off]),
        when: Kind::SpeedChanged,
        check: &Expr::And(
            &Expr::Node(&Duplicate),
            &Expr::And(&Expr::Node(&AlsoDuplicate), &Expr::Node(&StillDuplicate)),
        ),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Off),
    },
    Edge {
        id: "SILENT_INTERNAL",
        from: Source::These(&[Tag::Off]),
        when: Kind::PowerChanged,
        check: crate::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::Internal,
    },
    Edge {
        id: "NO_GUARD", // the id is already taken
        from: Source::These(&[Tag::Off]),
        when: Kind::SpeedChanged,
        check: crate::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Off),
    },
    Edge {
        id: "NO_GUARD", // and taken a third time, to be reported once
        from: Source::These(&[Tag::Off]),
        when: Kind::PowerChanged,
        check: crate::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Off),
    },
];

/// `GearChanged` is declared off limits in `Off`, but `NO_GUARD` handles it.
static BROKEN_IGNORES: &[Ignore<Broken>] = &[Ignore {
    from: Source::These(&[Tag::Off]),
    when: &[Kind::GearChanged],
    why: "test: contradicted by an edge",
}];

#[test]
fn coverage_reports_holes_unreachable_states_and_duplicate_names() {
    let c = verify::coverage::<Broken>(Tag::Off, BROKEN_EDGES, &[]);

    assert!(!c.is_clean());
    // Nothing is declared for Showing at all.
    assert!(
        c.holes
            .contains(&("Showing".to_owned(), "GearChanged".to_owned())),
        "{:?}",
        c.holes
    );
    assert_eq!(c.unreachable, vec!["Showing"]);
    // Reported once, however many types share the name.
    assert_eq!(c.duplicate_node_names, vec!["Duplicate"]);
}

/// The id has to survive reordering of the table, so two edges may not share it.
#[test]
fn coverage_reports_a_duplicate_edge_id() {
    let c = verify::coverage::<Broken>(Tag::Off, BROKEN_EDGES, &[]);

    assert_eq!(c.duplicate_edge_ids, vec!["NO_GUARD"]);
    assert!(!c.is_clean());
}

/// Each defect list vetoes `is_clean` on its own. A table carrying several at
/// once cannot show that, since the first empty check short-circuits the rest.
#[test]
fn is_clean_requires_every_defect_list_to_be_empty() {
    fn with(fill: impl FnOnce(&mut verify::Coverage)) -> verify::Coverage {
        let mut c = verify::Coverage::default();
        fill(&mut c);
        c
    }
    let state = || "Off".to_owned();
    let kind = || "GearChanged".to_owned();

    assert!(verify::Coverage::default().is_clean());

    assert!(!with(|c| c.holes.push((state(), kind()))).is_clean());
    assert!(!with(|c| c.ignored_but_handled.push((state(), kind(), vec!["E"]))).is_clean());
    assert!(!with(|c| c.unreachable.push(state())).is_clean());
    assert!(!with(|c| c.duplicate_node_names.push("Dup")).is_clean());
    assert!(!with(|c| c.duplicate_edge_ids.push("E")).is_clean());

    // A review signal, not a defect.
    assert!(with(|c| c.overlaps.push((state(), kind(), vec!["A", "B"]))).is_clean());
}

/// An `Ignore` bans a combination outright, so an edge on it is a defect however
/// the edge is guarded.
#[test]
fn coverage_reports_an_ignore_an_edge_contradicts() {
    let c = verify::coverage::<Broken>(Tag::Off, BROKEN_EDGES, BROKEN_IGNORES);

    assert_eq!(
        c.ignored_but_handled,
        vec![("Off".to_owned(), "GearChanged".to_owned(), vec!["NO_GUARD"])]
    );
    assert!(!c.is_clean());
}

#[test]
fn mermaid_labels_a_guardless_edge_and_its_run_actions() {
    let diagram = render::to_mermaid::<Broken>(Tag::Off, BROKEN_EDGES, BROKEN_STATES);

    assert!(
        diagram.contains("Off --> Off: GearChanged<br/>/ UpdateOverlay"),
        "{diagram}"
    );
    // Showing has no state table entry, so it gets no description line.
    assert!(!diagram.contains("Showing :"), "{diagram}");
}

#[test]
fn internal_table_dashes_an_empty_guard() {
    let table = render::internal_table::<Broken>(BROKEN_EDGES);

    assert!(table.contains("`SILENT_INTERNAL`"), "{table}");
    assert!(table.contains("`—`"), "{table}");
}

// ─────────────────────────────────────────── feature layer
// The same domain, driven without a transition table. `RearCam` has states, but
// nothing about `Feature` requires them — it only needs `Domain`. The guard
// nodes are the ones the machine above uses, which is the point of declaring
// them against the domain.

struct Camera;

impl Feature<RearCam> for Camera {
    const INFO: FeatureInfo<RearCam> = FeatureInfo {
        name: "Camera",
        rules: &[
            Rule {
                when: &[Kind::GearChanged],
                check: crate::check!(GearIsReverse),
                unknown: OnUnknown::Deny,
                emit: &[Action::ShowCamera],
            },
            Rule {
                when: &[Kind::GearChanged],
                check: crate::check!(),
                unknown: OnUnknown::Deny,
                emit: &[Action::HideCamera],
            },
        ],
    };
}

struct Overlay;

impl Feature<RearCam> for Overlay {
    const INFO: FeatureInfo<RearCam> = FeatureInfo {
        name: "Overlay",
        rules: &[Rule {
            when: &[Kind::SpeedChanged],
            check: crate::check!(SpeedBelowLimit),
            unknown: OnUnknown::Deny,
            emit: &[Action::UpdateOverlay],
        }],
    };
}

static CAMERA_FEATURES: &[FeatureInfo<RearCam>] = &[Camera::INFO, Overlay::INFO];

#[test]
fn dispatch_runs_only_the_declared_kinds() {
    let mut w = Env::default();

    // Declared: a rule matches and dispatch carries its effect out.
    feature::dispatch(&Camera::INFO, &Event::GearChanged(Gear::Reverse), &mut w);
    assert_eq!(w.performed, vec![Action::ShowCamera]);
    assert!(w.camera_visible);

    // Not declared: no rule is even considered, so no guard runs.
    w.speed = Some(10.0);
    feature::dispatch(&Camera::INFO, &Event::SpeedChanged, &mut w);
    assert_eq!(w.performed, vec![Action::ShowCamera]);
}

/// The event reaches `perform`, so an action may read a value off its payload
/// the way an edge's `run` actions can.
#[test]
fn dispatch_performs_with_the_event_that_caused_it() {
    let mut w = Env::default();

    feature::dispatch(&Camera::INFO, &Event::GearChanged(Gear::Drive), &mut w);

    assert_eq!(w.performed, vec![Action::HideCamera]);
    assert_eq!(w.performed_for, vec![Kind::GearChanged]);
}

/// Declaration order is priority: the guarded rule wins and the fallback below
/// it never runs.
#[test]
fn dispatch_takes_the_first_rule_that_matches() {
    let mut w = Env::default();

    feature::dispatch(&Camera::INFO, &Event::GearChanged(Gear::Reverse), &mut w);

    assert_eq!(w.performed, vec![Action::ShowCamera]);
}

/// An unguarded rule below a guarded one is the `else` branch.
#[test]
fn dispatch_falls_through_to_an_unguarded_rule() {
    let mut w = Env::default();

    feature::dispatch(&Camera::INFO, &Event::GearChanged(Gear::Drive), &mut w);

    assert_eq!(w.performed, vec![Action::HideCamera]);
}

/// `speed` is `None`, so the guard is `Unknown`. `Deny` holds the rule back and
/// nothing below it matches either.
#[test]
fn dispatch_denies_an_undecidable_guard() {
    let mut w = Env::default();

    feature::dispatch(&Overlay::INFO, &Event::SpeedChanged, &mut w);

    assert!(w.performed.is_empty());
}

/// The same feature, told to act when the lookup fails.
struct Optimist;

impl Feature<RearCam> for Optimist {
    const INFO: FeatureInfo<RearCam> = FeatureInfo {
        name: "Optimist",
        rules: &[Rule {
            when: &[Kind::SpeedChanged],
            check: crate::check!(SpeedBelowLimit),
            unknown: OnUnknown::Allow,
            emit: &[Action::UpdateOverlay],
        }],
    };
}

#[test]
fn dispatch_allows_an_undecidable_guard_when_told_to() {
    let mut w = Env::default();

    feature::dispatch(&Optimist::INFO, &Event::SpeedChanged, &mut w);

    assert_eq!(w.performed, vec![Action::UpdateOverlay]);
}

/// Two rules naming the same node. The `Memo` is per dispatch, so the lookup
/// happens once even though the node is reached twice.
struct Shared;

impl Feature<RearCam> for Shared {
    const INFO: FeatureInfo<RearCam> = FeatureInfo {
        name: "Shared",
        rules: &[
            Rule {
                when: &[Kind::SpeedChanged],
                check: crate::check!(SpeedBelowLimit && GearIsReverse),
                unknown: OnUnknown::Deny,
                emit: &[Action::ShowCamera],
            },
            Rule {
                when: &[Kind::SpeedChanged],
                check: crate::check!(SpeedBelowLimit),
                unknown: OnUnknown::Deny,
                emit: &[Action::UpdateOverlay],
            },
        ],
    };
}

#[test]
fn dispatch_evaluates_a_shared_node_once() {
    let mut w = Env {
        speed: Some(10.0),
        ..Default::default()
    };

    // The first rule fails on `GearIsReverse`, the second re-reads the speed
    // node and hits the cache.
    feature::dispatch(&Shared::INFO, &Event::SpeedChanged, &mut w);

    assert_eq!(w.performed, vec![Action::UpdateOverlay]);
    assert_eq!(w.speed_lookups.get(), 1);
}

/// Rules that overlap in both columns. `handles` and `emits` are the union, so
/// the summary table does not repeat itself.
struct Noisy;

impl Feature<RearCam> for Noisy {
    const INFO: FeatureInfo<RearCam> = FeatureInfo {
        name: "Noisy",
        rules: &[
            Rule {
                when: &[Kind::SpeedChanged, Kind::GearChanged],
                check: crate::check!(GearIsReverse),
                unknown: OnUnknown::Deny,
                emit: &[Action::ShowCamera, Action::ShowCamera],
            },
            Rule {
                when: &[Kind::GearChanged, Kind::PowerChanged],
                check: crate::check!(),
                unknown: OnUnknown::Deny,
                emit: &[Action::ShowCamera],
            },
        ],
    };
}

/// Kinds come back in the domain's declaration order, not the order the rules
/// happen to list them, so a reordered rule does not churn the document.
#[test]
fn handles_is_the_union_in_domain_order() {
    assert_eq!(
        Noisy::INFO.handles(),
        vec![Kind::GearChanged, Kind::SpeedChanged, Kind::PowerChanged]
    );
}

#[test]
fn emits_drops_repeats() {
    assert_eq!(Noisy::INFO.emits(), vec![Action::ShowCamera]);
}

#[test]
fn io_table_lists_each_feature() {
    let table = render::io_table(CAMERA_FEATURES);

    assert!(
        table.contains("| `Camera` | `GearChanged` | `ShowCamera`, `HideCamera` |"),
        "{table}"
    );
    assert!(
        table.contains("| `Overlay` | `SpeedChanged` | `UpdateOverlay` |"),
        "{table}"
    );
}

/// A column with nothing in it renders as a dash rather than an empty cell.
#[test]
fn io_table_dashes_a_feature_that_declares_nothing() {
    let idle: &[FeatureInfo<RearCam>] = &[FeatureInfo {
        name: "Idle",
        rules: &[],
    }];

    let table = render::io_table(idle);

    assert!(table.contains("| `Idle` | — | — |"), "{table}");
}

#[test]
fn rule_table_shows_the_guard_of_each_rule() {
    let table = render::rule_table(CAMERA_FEATURES);

    assert!(
        table.contains("| `Camera` | `GearChanged` | `GearIsReverse` | `ShowCamera` |"),
        "{table}"
    );
    assert!(
        table.contains("| `Overlay` | `SpeedChanged` | `SpeedBelowLimit` | `UpdateOverlay` |"),
        "{table}"
    );
}

/// An unguarded rule under a rule covering the same kind is a fallback, and is
/// labelled as one so the row does not read as unconditional.
#[test]
fn rule_table_calls_a_covered_unguarded_rule_else() {
    let table = render::rule_table(&[Camera::INFO]);

    assert!(
        table.contains("| `Camera` | `GearChanged` | else | `HideCamera` |"),
        "{table}"
    );
}

/// A rule with nothing before it really is unconditional, so it gets a dash.
#[test]
fn rule_table_dashes_a_rule_nothing_precedes() {
    let unconditional: &[FeatureInfo<RearCam>] = &[FeatureInfo {
        name: "Always",
        rules: &[Rule {
            when: &[Kind::PowerChanged],
            check: crate::check!(),
            unknown: OnUnknown::Deny,
            emit: &[Action::UpdateOverlay],
        }],
    }];

    let table = render::rule_table(unconditional);

    assert!(
        table.contains("| `Always` | `PowerChanged` | — | `UpdateOverlay` |"),
        "{table}"
    );
}

/// `unknown = Allow` is a decision the reader has to see, so it rides along in
/// the guard column.
#[test]
fn rule_table_marks_an_allowing_rule() {
    let table = render::rule_table(&[Optimist::INFO]);

    assert!(
        table.contains("`SpeedBelowLimit` (unknown=Allow)"),
        "{table}"
    );
}

#[test]
fn io_flowchart_keeps_features_and_actions_apart() {
    let chart = render::io_flowchart(CAMERA_FEATURES);

    assert!(chart.contains(r#"ev_GearChanged["GearChanged"] --> ft_Camera["Camera"]"#));
    assert!(
        chart.contains(r#"ft_Camera["Camera"] -->|"GearIsReverse"| ac_ShowCamera["ShowCamera"]"#)
    );
}

/// The arrow an action arrives by carries the condition that produced it, so
/// the diagram says why and not only whether.
#[test]
fn io_flowchart_labels_arrows_with_their_guard() {
    let chart = render::io_flowchart(&[Camera::INFO]);

    assert!(
        chart.contains(r#"ft_Camera["Camera"] -->|else| ac_HideCamera["HideCamera"]"#),
        "{chart}"
    );
}

/// An unconditional rule's arrow carries no label at all.
#[test]
fn io_flowchart_leaves_an_unconditional_arrow_bare() {
    let unconditional: &[FeatureInfo<RearCam>] = &[FeatureInfo {
        name: "Always",
        rules: &[Rule {
            when: &[Kind::PowerChanged],
            check: crate::check!(),
            unknown: OnUnknown::Deny,
            emit: &[Action::UpdateOverlay],
        }],
    }];

    let chart = render::io_flowchart(unconditional);

    assert!(
        chart.contains(r#"ft_Always["Always"] --> ac_UpdateOverlay["UpdateOverlay"]"#),
        "{chart}"
    );
}

#[test]
fn unhandled_kinds_reports_what_no_feature_takes() {
    // PowerChanged is in the event enum but no feature declares it.
    assert_eq!(
        feature::unhandled_kinds(CAMERA_FEATURES, &[]),
        vec![Kind::PowerChanged]
    );
}

/// A controller that mixes both layers is checked as one unit. An `Ignore` does
/// not make a kind handled — the table only says the machine has no use for it.
#[test]
fn unhandled_kinds_counts_edges_but_not_ignores() {
    let by_machine = verify::handled_kinds::<RearCam>(EDGES);

    assert!(by_machine.contains(&Kind::GearChanged));
    assert!(
        !by_machine.contains(&Kind::PowerChanged),
        "PowerChanged only has an Ignore"
    );
    assert_eq!(
        feature::unhandled_kinds(CAMERA_FEATURES, &[&by_machine]),
        vec![Kind::PowerChanged]
    );
}

#[test]
fn unemitted_actions_reports_what_no_feature_produces() {
    let only_camera = &[Camera::INFO];

    assert_eq!(
        feature::unemitted_actions(only_camera),
        vec![Action::UpdateOverlay]
    );
}
