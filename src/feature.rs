//! The stateless layer: one feature of a controller, declared as a table of
//! rules over its own effects.
//!
//! A [`Rule`] is one `input -> output` line: the event kinds it is considered
//! for, the condition that has to hold, and what it emits. Everything a feature
//! does is readable off its table, which is what the renderers and the coverage
//! helpers are derived from.
//!
//! This is [`crate::machine::Edge`] with the state axis removed: no `from`, no
//! `goto`, and `when` is a set because a feature rule routinely covers several
//! kinds at once.
//!
//! Each feature names its own [`Feature::Action`], so its [`Feature::perform`]
//! is exhaustive over exactly the effects it declares. [`AnyFeature`] is the one
//! uniform face a controller needs to walk them all despite that.

use std::any::TypeId;

use crate::guard::{Cx, Expr, Memo, OnUnknown};
use crate::{Domain, HasKind};

/// One rule of a feature: when it is considered, what must hold, and what it
/// emits.
///
/// When several rules match the same event, **declaration order is priority** —
/// the first is taken. A rule with an empty guard ([`crate::check!`] with no
/// nodes) always matches, which is how a fallback line is written.
///
/// `A` is the declaring feature's own action type, not a controller-wide one.
pub struct Rule<D: Domain, A: 'static> {
    /// Event kinds this rule is considered for.
    pub when: &'static [D::EventKind],
    /// The condition, over the event and the world.
    pub check: &'static Expr<D>,
    /// What to do when `check` is undecidable.
    pub unknown: OnUnknown,
    /// Actions to run, in order, when this rule is taken.
    pub emit: &'static [A],
}

/// One feature of a controller: its own effects, and the table that decides
/// between them.
///
/// The whole implementation is the table plus [`Feature::perform`]. There is no
/// handler, so a feature can do nothing the table does not say.
pub trait Feature: Sync + 'static {
    /// The vocabulary this feature works in. An associated type rather than a
    /// parameter, like [`crate::MachineSpec::Domain`]: a feature belongs to one
    /// controller.
    type Domain: Domain;

    /// This feature's effects. Its own type, so [`Feature::perform`] below is
    /// exhaustive over them and adding one is a compile error here and nowhere
    /// else. Bounded exactly like [`crate::MachineSpec::Action`];
    /// [`crate::verify::unemitted_actions`] asks for the rest where it needs it.
    type Action: Copy + std::fmt::Debug + 'static;

    /// Display name, used in tables and diagrams.
    const NAME: &'static str;

    /// The rules, in priority order.
    const RULES: &'static [Rule<Self::Domain, Self::Action>];

    /// Carries out one of this feature's actions — the only place it may mutate
    /// the world, since guards see `&Env`.
    ///
    /// - `action`: the effect to carry out.
    /// - `ev`: the event being dispatched, for effects that need a runtime
    ///   value from its payload.
    /// - `world`: the outside world to mutate.
    fn perform(
        action: Self::Action,
        ev: &<Self::Domain as Domain>::Event,
        world: &mut <Self::Domain as Domain>::Env,
    );
}

/// One rule, flattened for rendering.
///
/// Actions reach it as text: they are the one part of a feature the rest of the
/// controller has no type for.
pub struct RuleRow<D: Domain> {
    /// The kinds this rule is considered for.
    pub when: &'static [D::EventKind],
    /// The guard, rendered by [`Expr::render`]. Empty for an unguarded rule.
    pub guard: String,
    /// What the rule does when its guard is undecidable.
    pub unknown: OnUnknown,
    /// The actions it emits, in order.
    pub emit: Vec<String>,
}

/// The uniform face of a [`Feature`], so a controller can keep features of
/// different action types in one list and walk it.
///
/// Implemented for every [`Feature`] automatically; never implement it by hand.
/// It is what the router dispatches through and what the renderers read, so the
/// list a controller documents is the list it runs.
pub trait AnyFeature<D: Domain>: Sync {
    /// Display name, used in tables and diagrams.
    fn name(&self) -> &'static str;

    /// The event kinds some rule is considered for, without repeats, in
    /// [`Domain::all_kinds`] order rather than the order the rules happen to
    /// list them.
    fn handles(&self) -> Vec<D::EventKind>;

    /// The actions some rule may emit, in declaration order, without repeats.
    fn emits(&self) -> Vec<String>;

    /// Every rule, in priority order.
    fn rows(&self) -> Vec<RuleRow<D>>;

    /// Collects `(name, type id)` for every guard node this feature's rules
    /// reference.
    ///
    /// - `out`: pairs are appended here, for
    ///   [`crate::verify::duplicate_node_names`], which needs the type
    ///   alongside the name to tell a reused node from two nodes sharing a
    ///   name.
    fn node_ids(&self, out: &mut Vec<(&'static str, TypeId)>);

    /// Takes the first rule that matches `ev` and carries out its actions.
    ///
    /// - `ev`: the event to dispatch.
    /// - `world`: the outside world, read by the guards and mutated by the
    ///   actions.
    ///
    /// Guards only ever see `&Env` ([`Cx::world`]), so the only way a feature
    /// reaches the world is through [`Rule::emit`] and the
    /// [`Feature::perform`] it is handed to — which is why [`AnyFeature::emits`]
    /// can be trusted as the whole truth.
    ///
    /// A [`Memo`] is built per call, so a guard node shared by several rules is
    /// evaluated once per dispatch.
    ///
    /// Actions run before this returns, so a feature's effects land in `world`
    /// before the next feature is dispatched. Two features whose rules cover the
    /// same kind therefore see each other, in the order the caller walks them.
    fn dispatch(&self, ev: &D::Event, world: &mut D::Env);
}

impl<F: Feature> AnyFeature<F::Domain> for F {
    fn name(&self) -> &'static str {
        F::NAME
    }

    fn handles(&self) -> Vec<<F::Domain as Domain>::EventKind> {
        <F::Domain as Domain>::all_kinds()
            .iter()
            .copied()
            .filter(|k| F::RULES.iter().any(|r| r.when.contains(k)))
            .collect()
    }

    fn emits(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for a in F::RULES.iter().flat_map(|r| r.emit) {
            let name = format!("{a:?}");
            if !out.contains(&name) {
                out.push(name);
            }
        }
        out
    }

    fn rows(&self) -> Vec<RuleRow<F::Domain>> {
        F::RULES
            .iter()
            .map(|r| RuleRow {
                when: r.when,
                guard: r.check.render(),
                unknown: r.unknown,
                emit: r.emit.iter().map(|a| format!("{a:?}")).collect(),
            })
            .collect()
    }

    fn node_ids(&self, out: &mut Vec<(&'static str, TypeId)>) {
        for r in F::RULES {
            r.check.node_ids(out);
        }
    }

    fn dispatch(
        &self,
        ev: &<F::Domain as Domain>::Event,
        world: &mut <F::Domain as Domain>::Env,
    ) {
        let kind = ev.kind();

        // The borrow of `world` ends with `cx`, before the actions mutate it.
        let hit = {
            let memo = Memo::new();
            let cx = Cx::new(ev, &*world, &memo);
            F::RULES
                .iter()
                .position(|r| r.when.contains(&kind) && r.unknown.accepts(r.check.eval(&cx)))
        };

        let Some(hit) = hit else { return };
        let rule = &F::RULES[hit];

        for &a in rule.emit {
            F::perform(a, ev, world);
        }

        log::debug!("[chart] {}: {ev:?} -> {:?}", F::NAME, rule.emit);
    }
}
