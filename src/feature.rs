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

use crate::guard::{Cx, Expr, Memo, OnUnknown};
use crate::{Domain, Enumerable, HasKind};

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
pub trait Feature<D: Domain>: Sync + 'static {
    /// This feature's effects. Its own type, so [`Feature::perform`] below is
    /// exhaustive over them and adding one is a compile error here and nowhere
    /// else.
    type Action: Copy + PartialEq + std::fmt::Debug + 'static;

    /// Display name, used in tables and diagrams.
    const NAME: &'static str;

    /// The rules, in priority order.
    const RULES: &'static [Rule<D, Self::Action>];

    /// Carries out one of this feature's actions — the only place it may mutate
    /// the world, since guards see `&Env`.
    ///
    /// - `action`: the effect to carry out.
    /// - `ev`: the event being dispatched, for effects that need a runtime
    ///   value from its payload.
    /// - `world`: the outside world to mutate.
    fn perform(action: Self::Action, ev: &D::Event, world: &mut D::Env);
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

impl<D: Domain, F: Feature<D>> AnyFeature<D> for F {
    fn name(&self) -> &'static str {
        F::NAME
    }

    fn handles(&self) -> Vec<D::EventKind> {
        D::all_kinds()
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

    fn rows(&self) -> Vec<RuleRow<D>> {
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

    fn dispatch(&self, ev: &D::Event, world: &mut D::Env) {
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

/// Event kinds nothing in the controller accounts for.
///
/// - `features`: the controller's feature list.
/// - `elsewhere`: kinds handled outside that list, e.g.
///   [`crate::verify::handled_kinds`] for each state machine the controller also
///   runs; pass `&[]` if there are none.
///
/// Returns the event kinds handled by neither `features` nor `elsewhere`.
pub fn unhandled_kinds<D: Domain>(
    features: &[&dyn AnyFeature<D>],
    elsewhere: &[&[D::EventKind]],
) -> Vec<D::EventKind> {
    D::all_kinds()
        .iter()
        .copied()
        .filter(|k| !features.iter().any(|f| f.handles().contains(k)))
        .filter(|k| !elsewhere.iter().any(|ks| ks.contains(k)))
        .collect()
}

/// Actions `F` declares but no rule of `F` emits — dead effects.
///
/// One feature at a time rather than a whole controller at once: with an action
/// type per feature, an orphaned effect is a question about the file that owns
/// it.
///
/// Returns every value of `F::Action` missing from every [`Rule::emit`].
pub fn unemitted_actions<D: Domain, F: Feature<D>>() -> Vec<F::Action>
where
    F::Action: Enumerable,
{
    <F::Action as Enumerable>::ALL
        .iter()
        .copied()
        .filter(|a| !F::RULES.iter().any(|r| r.emit.contains(a)))
        .collect()
}
