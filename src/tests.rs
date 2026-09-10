//! Tests for the framework itself.
//!
//! Uses a reverse-camera controller, slightly richer than the README quick-start
//! example, to exercise every feature.

// The fixture's event names read better with the shared `Changed` suffix, and
// `events!` emits the same variant names for both enums.
#![allow(clippy::enum_variant_names)]

use std::cell::Cell;

use super::feature::{AnyFeature, Feature, Rule};
use super::guard::{Cond, Cx, Expr, Memo, OnUnknown};
use super::{Domain, Enumerable, HasKind, render, verify};

// ─────────────────────────────────────────── domain

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

// Only needed by `verify::unemitted_actions`; `Domain` does not require it.
impl Enumerable for Action {
    const ALL: &'static [Self] = &[Self::ShowCamera, Self::HideCamera, Self::UpdateOverlay];
}

#[derive(Default)]
struct World {
    /// `None` models a failed lookup, which yields `Cond::Unknown`.
    speed: Option<f32>,
    camera_visible: bool,
    performed: Vec<Action>,
    /// The event kind each action was performed for, recorded from `perform`'s
    /// `ev` argument.
    performed_for: Vec<Kind>,
    /// How many times `SpeedBelowLimit` looked the speed up. Guards receive
    /// `&World`, so this needs interior mutability.
    speed_lookups: Cell<u32>,
}

struct RearCam;

impl Domain for RearCam {
    type Event = Event;
    type EventKind = Kind;
    type World = World;
}

/// Every feature in this fixture runs its effects the same way, so they all
/// point here. Splitting effects per feature is what the example does; the
/// tests are about dispatch, not about who owns what.
fn perform_action(action: Action, world: &mut World) {
    world.performed.push(action);
    match action {
        Action::ShowCamera => world.camera_visible = true,
        Action::HideCamera => world.camera_visible = false,
        Action::UpdateOverlay => {}
    }
}

/// Records the kind the action was performed for.
fn perform_for_event(action: Action, ev: &Event, world: &mut World) {
    world.performed_for.push(ev.kind());
    perform_action(action, world);
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

// ─────────────────────────────────────────── feature layer
// The same domain, driven without a transition table. `RearCam` has states, but
// nothing about `Feature` requires them — it only needs `Domain`. The guard
// nodes are the ones the machine above uses, which is the point of declaring
// them against the domain.
//
// Each fixture names its own action type. They all speak to `World` through the
// same helpers, because the world has one language even when its callers do
// not.

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum CamAction {
    ShowCamera,
    HideCamera,
}

// Only needed by `feature::unemitted_actions`; `Feature` does not require it.
impl Enumerable for CamAction {
    const ALL: &'static [Self] = &[Self::ShowCamera, Self::HideCamera];
}

struct Camera;

impl Feature for Camera {
    type Domain = RearCam;
    type Action = CamAction;

    const NAME: &'static str = "Camera";

    const RULES: &'static [Rule<RearCam, CamAction>] = &[
        Rule {
            id: "CAMERA_1",
            when: &[Kind::GearChanged],
            check: crate::check!(GearIsReverse),
            unknown: OnUnknown::Deny,
            emit: &[CamAction::ShowCamera],
        },
        Rule {
            id: "CAMERA_2",
            when: &[Kind::GearChanged],
            check: crate::check!(),
            unknown: OnUnknown::Deny,
            emit: &[CamAction::HideCamera],
        },
    ];

    fn perform(action: CamAction, ev: &Event, world: &mut World) {
        perform_for_event(
            match action {
                CamAction::ShowCamera => Action::ShowCamera,
                CamAction::HideCamera => Action::HideCamera,
            },
            ev,
            world,
        );
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum OverlayAction {
    UpdateOverlay,
}

impl Enumerable for OverlayAction {
    const ALL: &'static [Self] = &[Self::UpdateOverlay];
}

struct Overlay;

impl Feature for Overlay {
    type Domain = RearCam;
    type Action = OverlayAction;

    const NAME: &'static str = "Overlay";

    const RULES: &'static [Rule<RearCam, OverlayAction>] = &[Rule {
        id: "OVERLAY_1",
        when: &[Kind::SpeedChanged],
        check: crate::check!(SpeedBelowLimit),
        unknown: OnUnknown::Deny,
        emit: &[OverlayAction::UpdateOverlay],
    }];

    fn perform(action: OverlayAction, ev: &Event, world: &mut World) {
        match action {
            OverlayAction::UpdateOverlay => perform_for_event(Action::UpdateOverlay, ev, world),
        }
    }
}

static CAMERA_FEATURES: &[&dyn AnyFeature<RearCam>] = &[&Camera, &Overlay];

#[test]
fn dispatch_runs_only_the_declared_kinds() {
    let mut w = World::default();

    // Declared: a rule matches and dispatch carries its effect out.
    Camera.dispatch(&Event::GearChanged(Gear::Reverse), &mut w);
    assert_eq!(w.performed, vec![Action::ShowCamera]);
    assert!(w.camera_visible);

    // Not declared: no rule is even considered, so no guard runs.
    w.speed = Some(10.0);
    Camera.dispatch(&Event::SpeedChanged, &mut w);
    assert_eq!(w.performed, vec![Action::ShowCamera]);
}

/// The event reaches `perform`, so an action may read a value off its payload
/// the way an edge's `run` actions can.
#[test]
fn dispatch_performs_with_the_event_that_caused_it() {
    let mut w = World::default();

    Camera.dispatch(&Event::GearChanged(Gear::Drive), &mut w);

    assert_eq!(w.performed, vec![Action::HideCamera]);
    assert_eq!(w.performed_for, vec![Kind::GearChanged]);
}

/// Declaration order is priority: the guarded rule wins and the fallback below
/// it never runs.
#[test]
fn dispatch_takes_the_first_rule_that_matches() {
    let mut w = World::default();

    Camera.dispatch(&Event::GearChanged(Gear::Reverse), &mut w);

    assert_eq!(w.performed, vec![Action::ShowCamera]);
}

/// An unguarded rule below a guarded one is the `else` branch.
#[test]
fn dispatch_falls_through_to_an_unguarded_rule() {
    let mut w = World::default();

    Camera.dispatch(&Event::GearChanged(Gear::Drive), &mut w);

    assert_eq!(w.performed, vec![Action::HideCamera]);
}

/// `speed` is `None`, so the guard is `Unknown`. `Deny` holds the rule back and
/// nothing below it matches either.
#[test]
fn dispatch_denies_an_undecidable_guard() {
    let mut w = World::default();

    Overlay.dispatch(&Event::SpeedChanged, &mut w);

    assert!(w.performed.is_empty());
}

/// The same feature, told to act when the lookup fails.
struct Optimist;

impl Feature for Optimist {
    type Domain = RearCam;
    type Action = OverlayAction;

    const NAME: &'static str = "Optimist";

    const RULES: &'static [Rule<RearCam, OverlayAction>] = &[Rule {
        id: "OPTIMIST_1",
        when: &[Kind::SpeedChanged],
        check: crate::check!(SpeedBelowLimit),
        unknown: OnUnknown::Allow,
        emit: &[OverlayAction::UpdateOverlay],
    }];

    fn perform(action: OverlayAction, ev: &Event, world: &mut World) {
        Overlay::perform(action, ev, world);
    }
}

#[test]
fn dispatch_allows_an_undecidable_guard_when_told_to() {
    let mut w = World::default();

    Optimist.dispatch(&Event::SpeedChanged, &mut w);

    assert_eq!(w.performed, vec![Action::UpdateOverlay]);
}

/// Two rules naming the same node. The `Memo` is per dispatch, so the lookup
/// happens once even though the node is reached twice.
struct Shared;

impl Feature for Shared {
    type Domain = RearCam;
    type Action = CamAction;

    const NAME: &'static str = "Shared";

    const RULES: &'static [Rule<RearCam, CamAction>] = &[
        Rule {
            id: "SHARED_1",
            when: &[Kind::SpeedChanged],
            check: crate::check!(SpeedBelowLimit && GearIsReverse),
            unknown: OnUnknown::Deny,
            emit: &[CamAction::ShowCamera],
        },
        Rule {
            id: "SHARED_2",
            when: &[Kind::SpeedChanged],
            check: crate::check!(SpeedBelowLimit),
            unknown: OnUnknown::Deny,
            emit: &[CamAction::HideCamera],
        },
    ];

    fn perform(action: CamAction, ev: &Event, world: &mut World) {
        Camera::perform(action, ev, world);
    }
}

#[test]
fn dispatch_evaluates_a_shared_node_once() {
    let mut w = World {
        speed: Some(10.0),
        ..Default::default()
    };

    // The first rule fails on `GearIsReverse`, the second re-reads the speed
    // node and hits the cache.
    Shared.dispatch(&Event::SpeedChanged, &mut w);

    assert_eq!(w.performed, vec![Action::HideCamera]);
    assert_eq!(w.speed_lookups.get(), 1);
}

/// An action carrying a value, whose `{:?}` puts punctuation in the middle of
/// what becomes a node id.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum PayloadAction {
    Show(u8),
}

struct Payload;

impl Feature for Payload {
    type Domain = RearCam;
    type Action = PayloadAction;

    const NAME: &'static str = "Payload";

    const RULES: &'static [Rule<RearCam, PayloadAction>] = &[Rule {
        id: "PAYLOAD_1",
        when: &[Kind::GearChanged],
        check: crate::check!(GearIsReverse),
        unknown: OnUnknown::Deny,
        emit: &[PayloadAction::Show(7)],
    }];

    fn perform(action: PayloadAction, ev: &Event, world: &mut World) {
        match action {
            PayloadAction::Show(_) => perform_for_event(Action::ShowCamera, ev, world),
        }
    }
}

/// Rules that overlap in both columns. `handles` and `emits` are the union, so
/// the summary table does not repeat itself.
struct Noisy;

impl Feature for Noisy {
    type Domain = RearCam;
    type Action = CamAction;

    const NAME: &'static str = "Noisy";

    const RULES: &'static [Rule<RearCam, CamAction>] = &[
        Rule {
            id: "NOISY_1",
            when: &[Kind::SpeedChanged, Kind::GearChanged],
            check: crate::check!(GearIsReverse),
            unknown: OnUnknown::Deny,
            emit: &[CamAction::ShowCamera, CamAction::ShowCamera],
        },
        Rule {
            id: "NOISY_2",
            when: &[Kind::GearChanged, Kind::PowerChanged],
            check: crate::check!(),
            unknown: OnUnknown::Deny,
            emit: &[CamAction::ShowCamera],
        },
    ];

    fn perform(action: CamAction, ev: &Event, world: &mut World) {
        Camera::perform(action, ev, world);
    }
}

/// Kinds come back in the domain's declaration order, not the order the rules
/// happen to list them, so a reordered rule does not churn the document.
#[test]
fn handles_is_the_union_in_domain_order() {
    assert_eq!(
        Noisy.handles(),
        vec![Kind::GearChanged, Kind::SpeedChanged, Kind::PowerChanged]
    );
}

#[test]
fn emits_drops_repeats() {
    assert_eq!(Noisy.emits(), vec!["ShowCamera".to_string()]);
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

/// A feature with no rules at all: both columns render as a dash rather than an
/// empty cell.
struct Idle;

impl Feature for Idle {
    type Domain = RearCam;
    type Action = CamAction;

    const NAME: &'static str = "Idle";

    const RULES: &'static [Rule<RearCam, CamAction>] = &[];

    fn perform(action: CamAction, ev: &Event, world: &mut World) {
        Camera::perform(action, ev, world);
    }
}

#[test]
fn io_table_dashes_a_feature_that_declares_nothing() {
    let table = render::io_table(&[&Idle]);

    assert!(table.contains("| `Idle` | — | — |"), "{table}");
}

#[test]
fn rule_table_shows_the_guard_of_each_rule() {
    let table = render::rule_table(CAMERA_FEATURES);

    assert!(
        table.contains("| `Camera` | `CAMERA_1` | `GearChanged` | `GearIsReverse` | `ShowCamera` |"),
        "{table}"
    );
    assert!(
        table.contains("| `Overlay` | `OVERLAY_1` | `SpeedChanged` | `SpeedBelowLimit` | `UpdateOverlay` |"),
        "{table}"
    );
}

/// An unguarded rule under a rule covering the same kind is a fallback, and is
/// labelled as one so the row does not read as unconditional.
#[test]
fn rule_table_calls_a_covered_unguarded_rule_else() {
    let table = render::rule_table(&[&Camera]);

    assert!(
        table.contains("| `Camera` | `CAMERA_2` | `GearChanged` | else | `HideCamera` |"),
        "{table}"
    );
}

/// A rule with nothing before it really is unconditional, so it gets a dash.
struct Always;

impl Feature for Always {
    type Domain = RearCam;
    type Action = OverlayAction;

    const NAME: &'static str = "Always";

    const RULES: &'static [Rule<RearCam, OverlayAction>] = &[Rule {
        id: "ALWAYS_1",
        when: &[Kind::PowerChanged],
        check: crate::check!(),
        unknown: OnUnknown::Deny,
        emit: &[OverlayAction::UpdateOverlay],
    }];

    fn perform(action: OverlayAction, ev: &Event, world: &mut World) {
        Overlay::perform(action, ev, world);
    }
}

#[test]
fn rule_table_dashes_a_rule_nothing_precedes() {
    let table = render::rule_table(&[&Always]);

    assert!(
        table.contains("| `Always` | `ALWAYS_1` | `PowerChanged` | — | `UpdateOverlay` |"),
        "{table}"
    );
}

/// `unknown = Allow` is a decision the reader has to see, so it rides along in
/// the guard column.
#[test]
fn rule_table_marks_an_allowing_rule() {
    let table = render::rule_table(&[&Optimist]);

    assert!(
        table.contains("`SpeedBelowLimit` (unknown=Allow)"),
        "{table}"
    );
}

// ─────────────────────────────────────────── one event at a time

/// One signal's whole answer, features included, and nothing else.
#[test]
fn event_table_gathers_one_kind_across_features() {
    let table = render::event_table(CAMERA_FEATURES, Kind::GearChanged);

    assert!(table.contains("`CAMERA_1`"), "{table}");
    assert!(table.contains("`CAMERA_2`"), "{table}");
    // Overlay reacts to SpeedChanged alone, so it contributes no row here.
    assert!(!table.contains("`OVERLAY_1`"), "{table}");
}

/// The `when` column survives the filter, so a rule appearing under two signals
/// says why.
#[test]
fn event_table_keeps_the_whole_trigger_set() {
    let table = render::event_table(&[&Noisy], Kind::PowerChanged);

    assert!(
        table.contains("| `Noisy` | `NOISY_2` | `GearChanged`, `PowerChanged` |"),
        "{table}"
    );
}

/// Shadowing is a question about one signal, not about a feature: `NOISY_2` is
/// `NOISY_1`'s fallback on `GearChanged` and unconditional on `PowerChanged`.
#[test]
fn event_table_decides_else_against_the_event_not_the_feature() {
    let gear = render::event_table(&[&Noisy], Kind::GearChanged);
    let power = render::event_table(&[&Noisy], Kind::PowerChanged);

    assert!(gear.contains("| `NOISY_2` | `GearChanged`, `PowerChanged` | else |"), "{gear}");
    assert!(power.contains("| `NOISY_2` | `GearChanged`, `PowerChanged` | — |"), "{power}");
}

#[test]
fn event_table_is_empty_for_a_kind_nothing_takes() {
    let table = render::event_table(CAMERA_FEATURES, Kind::PowerChanged);

    assert!(table.lines().count() == 2, "header only, got:\n{table}");
}

/// Only the features and rules that react to this kind appear.
#[test]
fn event_flowchart_draws_one_kind_and_its_features() {
    let declared = render::event_flowchart(CAMERA_FEATURES, Kind::GearChanged);

    assert!(declared.contains(r#"ev_GearChanged["GearChanged"] --> ft_Camera["Camera"]"#));
    assert!(
        declared.contains(
            r#"ft_Camera["Camera"] -->|"CAMERA_1<br/>GearIsReverse"| ac_Camera_ShowCamera["ShowCamera"]"#
        ),
        "{declared}"
    );
    assert!(!declared.contains("ft_Overlay"), "{declared}");
    assert!(!declared.contains("ev_SpeedChanged"), "{declared}");
}

/// An action node id carries its feature: two features that name an effect
/// alike are drawing two different effects, so the nodes must not merge.
#[test]
fn event_flowchart_scopes_action_nodes_to_their_feature() {
    let declared = render::event_flowchart(&[&Camera, &Noisy], Kind::GearChanged);

    assert!(declared.contains("ac_Camera_ShowCamera"), "{declared}");
    assert!(declared.contains("ac_Noisy_ShowCamera"), "{declared}");
}

/// An arrow names the rule that produced the action, then its guard.
#[test]
fn event_flowchart_labels_arrows_with_their_rule_and_guard() {
    let declared = render::event_flowchart(&[&Camera], Kind::GearChanged);

    assert!(
        declared.contains(
            r#"ft_Camera["Camera"] -->|"CAMERA_2<br/>else"| ac_Camera_HideCamera["HideCamera"]"#
        ),
        "{declared}"
    );
}

/// An unconditional rule's arrow carries its id and nothing else.
#[test]
fn event_flowchart_labels_an_unconditional_arrow_with_the_id_alone() {
    let declared = render::event_flowchart(&[&Always], Kind::PowerChanged);

    assert!(
        declared.contains(
            r#"ft_Always["Always"] -->|"ALWAYS_1"| ac_Always_UpdateOverlay["UpdateOverlay"]"#
        ),
        "{declared}"
    );
}

/// A node id may not carry punctuation — mermaid ends the identifier at `(`
/// and fails to parse the line. The quoted label keeps it.
#[test]
fn event_flowchart_folds_punctuation_out_of_node_ids() {
    let declared = render::event_flowchart(&[&Payload], Kind::GearChanged);

    assert!(
        declared.contains(r#"ac_Payload_Show_7_["Show(7)"]"#),
        "{declared}"
    );
    assert!(!declared.contains("ac_Payload_Show(7)"), "{declared}");
}

#[test]
fn event_flowchart_is_bare_for_a_kind_nothing_takes() {
    assert_eq!(
        render::event_flowchart(CAMERA_FEATURES, Kind::PowerChanged),
        "flowchart LR\n"
    );
}

#[test]
fn unhandled_kinds_reports_what_no_feature_takes() {
    // PowerChanged is in the event enum but no feature declares it.
    assert_eq!(
        verify::unhandled_kinds(CAMERA_FEATURES),
        vec![Kind::PowerChanged]
    );
}

/// An effect a feature declares but never emits is dead, and the check is a
/// question about that one feature: with an action type per feature, nobody
/// else could have emitted it.
#[test]
fn unemitted_actions_reports_what_a_feature_never_produces() {
    assert_eq!(
        verify::unemitted_actions::<Overlay>(),
        Vec::<OverlayAction>::new()
    );
    assert_eq!(
        verify::unemitted_actions::<Always>(),
        Vec::<OverlayAction>::new()
    );
    // `Shared` emits both of `CamAction`'s variants; `Idle` emits neither.
    assert_eq!(
        verify::unemitted_actions::<Idle>(),
        vec![CamAction::ShowCamera, CamAction::HideCamera]
    );
}

// ─────────────────────────────────────────── guard names across both layers
// The defect `duplicate_node_names` exists for: two node *types* answering to
// one name. `cond_node!` takes the name from the identifier, so this needs two
// modules — which is exactly the drift the check is meant to catch.

mod elsewhere {
    use super::{Cond, RearCam};

    // Same identifier as the `SpeedBelowLimit` above, declared somewhere else
    // and meaning something else. Both report "SpeedBelowLimit".
    crate::cond_node!(RearCam, SpeedBelowLimit, |_cx| Cond::True);
}

// How the collision actually reaches one rule list: `check!` takes an
// identifier, so a node from another module has to be aliased on the way in —
// and the alias renames the binding, never `name()`.
use elsewhere::SpeedBelowLimit as Other;

/// A feature whose two rules reach two different nodes sharing a name. Within
/// one dispatch they share a `Memo` entry, so the second inherits the first's
/// answer.
struct Colliding;

impl Feature for Colliding {
    type Domain = RearCam;
    type Action = OverlayAction;

    const NAME: &'static str = "Colliding";

    const RULES: &'static [Rule<RearCam, OverlayAction>] = &[
        Rule {
            id: "COLLIDING_1",
            when: &[Kind::SpeedChanged],
            check: crate::check!(SpeedBelowLimit),
            unknown: OnUnknown::Deny,
            emit: &[OverlayAction::UpdateOverlay],
        },
        Rule {
            id: "COLLIDING_2",
            when: &[Kind::SpeedChanged],
            check: crate::check!(Other),
            unknown: OnUnknown::Deny,
            emit: &[OverlayAction::UpdateOverlay],
        },
    ];

    fn perform(action: OverlayAction, ev: &Event, world: &mut World) {
        Overlay::perform(action, ev, world);
    }
}

/// Reuse is the normal case and must not be reported: `Shared` names one node
/// on two rules, and `CAMERA_FEATURES` spreads nodes across two features.
#[test]
fn duplicate_node_names_passes_a_node_reused_across_rules_and_features() {
    assert!(verify::duplicate_node_names(&[&Shared]).is_empty());
    assert!(verify::duplicate_node_names(CAMERA_FEATURES).is_empty());
}

/// Two types, one name. Nothing in the feature layer caught this before, and
/// the second node's `eval` never runs to reveal it.
#[test]
fn duplicate_node_names_reports_two_types_sharing_a_name() {
    assert_eq!(
        verify::duplicate_node_names(&[&Colliding]),
        vec!["SpeedBelowLimit"]
    );
}

/// The collision no single feature can see: each names one of the two nodes, so
/// each is clean alone. Only a scan across the controller finds it.
#[test]
fn duplicate_node_names_spans_two_features() {
    // `Colliding2` names only `elsewhere`'s node, `Overlay` only the original.
    assert!(verify::duplicate_node_names(&[&Colliding2]).is_empty());
    assert!(verify::duplicate_node_names(&[&Overlay]).is_empty());

    assert_eq!(
        verify::duplicate_node_names(&[&Colliding2, &Overlay]),
        vec!["SpeedBelowLimit"]
    );
}

/// Names only `elsewhere`'s node, so it is clean on its own.
struct Colliding2;

impl Feature for Colliding2 {
    type Domain = RearCam;
    type Action = OverlayAction;

    const NAME: &'static str = "Colliding2";

    const RULES: &'static [Rule<RearCam, OverlayAction>] = &[Rule {
        id: "COLLIDING2_1",
        when: &[Kind::SpeedChanged],
        check: crate::check!(Other),
        unknown: OnUnknown::Deny,
        emit: &[OverlayAction::UpdateOverlay],
    }];

    fn perform(action: OverlayAction, ev: &Event, world: &mut World) {
        Overlay::perform(action, ev, world);
    }
}

/// The consequence the check is standing in for: with two nodes sharing a name,
/// the second rule never evaluates its own guard.
#[test]
fn a_shared_name_makes_the_second_node_inherit_the_first_answer() {
    let mut w = World {
        // `SpeedBelowLimit` is False here; `elsewhere`'s node is always True,
        // so rule two would emit if it were the one being asked.
        speed: Some(200.0),
        ..Default::default()
    };

    Colliding.dispatch(&Event::SpeedChanged, &mut w);

    assert!(
        w.performed.is_empty(),
        "rule two inherited the cached False: {:?}",
        w.performed
    );
    // One lookup, not two: the second node's `eval` was never reached.
    assert_eq!(w.speed_lookups.get(), 1);
}

// ─────────────────────────────────────────── dispatch reports what ran

/// The feature layer's answer to `Taken::edge`: the id of the rule that ran.
/// Only the id, since this arrives through `&dyn AnyFeature`.
#[test]
fn feature_dispatch_returns_the_rule_that_ran() {
    let mut w = World::default();

    assert_eq!(
        Camera.dispatch(&Event::GearChanged(Gear::Reverse), &mut w),
        Some("CAMERA_1")
    );
    // The fallback below it, when the guard above does not hold.
    assert_eq!(
        Camera.dispatch(&Event::GearChanged(Gear::Drive), &mut w),
        Some("CAMERA_2")
    );
    // A kind no rule is considered for.
    assert_eq!(Camera.dispatch(&Event::PowerChanged, &mut w), None);
}

/// The id names one rule across the controller, because `dispatch` hands it
/// back with no feature attached.
#[test]
fn duplicate_rule_ids_reports_a_repeat_across_features() {
    assert!(verify::duplicate_rule_ids(CAMERA_FEATURES).is_empty());

    // `Camera` and `Twin` both declare `CAMERA_1`.
    struct Twin;
    impl Feature for Twin {
        type Domain = RearCam;
        type Action = CamAction;
        const NAME: &'static str = "Twin";
        const RULES: &'static [Rule<RearCam, CamAction>] = &[Rule {
            id: "CAMERA_1",
            when: &[Kind::PowerChanged],
            check: crate::check!(),
            unknown: OnUnknown::Deny,
            emit: &[CamAction::ShowCamera],
        }];
        fn perform(action: CamAction, ev: &Event, world: &mut World) {
            Camera::perform(action, ev, world);
        }
    }

    assert_eq!(
        verify::duplicate_rule_ids(&[&Camera, &Twin]),
        vec!["CAMERA_1"]
    );
}

/// A guard saying no is not the same as nothing covering the event. `Optimist`
/// reacts to `SpeedChanged`, so the kind is handled; the rule just declines.
#[test]
fn a_declining_guard_is_not_a_missing_rule() {
    let mut w = World {
        speed: Some(200.0),
        ..Default::default()
    };

    assert!(Optimist.dispatch(&Event::SpeedChanged, &mut w).is_none());
    assert!(w.performed.is_empty());
    assert!(Optimist.handles().contains(&Kind::SpeedChanged));
}

// ─────────────────────────────────────────── guard expressions
// The tree itself, independent of any table that holds one.

/// `Not` is parenthesised so `!(A && B)` cannot be read as `!A && B`.
#[test]
fn render_parenthesises_negated_subexpressions() {
    let inner: &'static Expr<RearCam> = crate::check!(GearIsReverse && SpeedBelowLimit);
    let negated = Expr::Not(inner);

    assert_eq!(negated.render(), "!(GearIsReverse && SpeedBelowLimit)");
    assert_eq!(
        Expr::<RearCam>::Not(crate::check!(GearIsReverse)).render(),
        "!GearIsReverse"
    );
}

/// An empty `check!` is always true, renders to nothing, and names no node — so
/// an unguarded rule adds no column and no name to any report.
#[test]
fn always_is_true_renders_empty_and_references_no_nodes() {
    let always: &'static Expr<RearCam> = crate::check!();
    let w = World::default();
    let memo = Memo::new();
    let cx = Cx::new(&Event::SpeedChanged, &w, &memo);

    assert_eq!(always.eval(&cx), Cond::True);
    assert_eq!(always.render(), "");

    let mut ids = Vec::new();
    always.node_ids(&mut ids);
    assert!(ids.is_empty());
}

/// `Or` stops at the first `True`, so the right side is never asked.
#[test]
fn or_short_circuits_on_true() {
    let w = World {
        // Would be Unknown if it were reached.
        speed: None,
        ..Default::default()
    };
    let memo = Memo::new();
    let cx = Cx::new(&Event::GearChanged(Gear::Reverse), &w, &memo);
    let expr: Expr<RearCam> = Expr::Or(
        crate::check!(GearIsReverse),
        crate::check!(SpeedBelowLimit),
    );

    assert_eq!(expr.eval(&cx), Cond::True);
    assert_eq!(w.speed_lookups.get(), 0, "right side was evaluated");
}

/// `And` stops at the first `False`, likewise.
#[test]
fn and_short_circuits_on_false() {
    let w = World {
        speed: None,
        ..Default::default()
    };
    let memo = Memo::new();
    let cx = Cx::new(&Event::GearChanged(Gear::Drive), &w, &memo);
    let expr: Expr<RearCam> = Expr::And(
        crate::check!(GearIsReverse),
        crate::check!(SpeedBelowLimit),
    );

    assert_eq!(expr.eval(&cx), Cond::False);
    assert_eq!(w.speed_lookups.get(), 0, "right side was evaluated");
}

// ─────────────────────────────────────────── generated declarations

/// `events!` produces the kind list the reports walk, complete by construction.
#[test]
fn events_generates_an_exhaustive_kind_list() {
    assert_eq!(
        <Kind as Enumerable>::ALL,
        &[Kind::GearChanged, Kind::SpeedChanged, Kind::PowerChanged]
    );
}

/// One `HasKind` arm per variant, whether or not the variant carries a payload.
#[test]
fn generated_kind_maps_payload_and_unit_variants() {
    assert_eq!(Event::GearChanged(Gear::Reverse).kind(), Kind::GearChanged);
    assert_eq!(Event::SpeedChanged.kind(), Kind::SpeedChanged);
}

/// Dispatch is not re-entrant by design: a rule's effects land in `World`
/// before the next event is looked at, so a follow-up event is queued by the
/// caller and handled in turn.
#[test]
fn caller_side_queue_processes_events_in_order() {
    let mut w = World::default();
    let queue = [Event::GearChanged(Gear::Reverse), Event::GearChanged(Gear::Drive)];

    for ev in &queue {
        Camera.dispatch(ev, &mut w);
    }

    assert_eq!(
        w.performed,
        vec![Action::ShowCamera, Action::HideCamera],
        "each event was handled to completion before the next"
    );
    assert!(!w.camera_visible);
}
