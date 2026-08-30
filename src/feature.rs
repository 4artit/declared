//! The stateless layer: one feature of a controller, declared as a table of
//! rules rather than as a handler.
//!
//! A [`Rule`] is one `input -> output` line: the event kinds it is considered
//! for, the condition that has to hold, and what it emits. Everything a
//! feature does is therefore readable off its table, which is what
//! [`FeatureInfo::handles`], [`FeatureInfo::emits`] and the renderers are
//! derived from.
//!
//! This is [`crate::machine::Edge`] with the state axis removed: no `from`, no
//! `goto`, and `when` is a set because a feature rule routinely covers several
//! kinds at once.

use crate::guard::{Cx, Expr, Memo, OnUnknown};
use crate::{Domain, Enumerable, HasKind};

/// One rule of a feature: when it is considered, what must hold, and what it
/// emits.
///
/// When several rules match the same event, **declaration order is priority** —
/// [`dispatch`] takes the first. A rule with an empty guard
/// ([`crate::check!`] with no nodes) always matches, which is how a fallback
/// line is written.
pub struct Rule<D: Domain> {
    /// Event kinds this rule is considered for.
    pub when: &'static [D::EventKind],
    /// The condition, over the event and the world.
    pub check: &'static Expr<D>,
    /// What to do when `check` is undecidable.
    pub unknown: OnUnknown,
    /// Actions to run, in order, when this rule is taken.
    pub emit: &'static [D::Action],
}

/// One feature's whole declaration: its name and its rules.
///
/// A plain value, so features can be collected into a slice for
/// [`crate::render::io_table`]. It carries no handler, so there is nothing a
/// feature can do that is not in `rules`.
pub struct FeatureInfo<D: Domain> {
    /// Display name, used in tables and diagrams.
    pub name: &'static str,
    /// The rules, in priority order.
    pub rules: &'static [Rule<D>],
}

impl<D: Domain> FeatureInfo<D> {
    /// The event kinds some rule is considered for, without repeats.
    ///
    /// Returns them in [`Domain::all_kinds`] order rather than the order the
    /// rules happen to list them, so the tables read the same way every time.
    pub fn handles(&self) -> Vec<D::EventKind> {
        D::all_kinds()
            .iter()
            .copied()
            .filter(|k| self.rules.iter().any(|r| r.when.contains(k)))
            .collect()
    }
}

impl<D: Domain> FeatureInfo<D>
where
    D::Action: PartialEq,
{
    /// The actions some rule may emit, in declaration order, without repeats.
    pub fn emits(&self) -> Vec<D::Action> {
        let mut out: Vec<D::Action> = Vec::new();
        for a in self.rules.iter().flat_map(|r| r.emit) {
            if !out.contains(a) {
                out.push(*a);
            }
        }
        out
    }
}

/// One feature of a controller. The whole implementation is the table.
pub trait Feature<D: Domain> {
    /// This feature's rules, and the name they are drawn under.
    const INFO: FeatureInfo<D>;
}

/// Takes the first rule of `f` that matches `ev` and carries out its actions.
///
/// - `f`: the feature to run.
/// - `ev`: the event to dispatch.
/// - `world`: the outside world, read by the guards and mutated by the actions.
///
/// Guards only ever see `&Env` ([`Cx::world`]), so the only way a feature
/// reaches the world is through [`Rule::emit`] — which is why
/// [`FeatureInfo::emits`] can be trusted as the whole truth.
///
/// A [`Memo`] is built per call, so a guard node shared by several rules, or by
/// a machine dispatched for the same event, is evaluated once per dispatch.
///
/// Actions run before this returns, so a feature's effects land in `world`
/// before the next feature is dispatched. Two features whose rules cover the
/// same kind therefore see each other, in the order the caller dispatches them.
pub fn dispatch<D: Domain>(f: &FeatureInfo<D>, ev: &D::Event, world: &mut D::Env) {
    let kind = ev.kind();

    // The borrow of `world` ends with `cx`, before the actions mutate it.
    let hit = {
        let memo = Memo::new();
        let cx = Cx::new(ev, &*world, &memo);
        f.rules
            .iter()
            .position(|r| r.when.contains(&kind) && r.unknown.accepts(r.check.eval(&cx)))
    };

    let Some(hit) = hit else { return };
    let rule = &f.rules[hit];

    for &a in rule.emit {
        D::perform(a, ev, world);
    }

    log::debug!("[chart] {}: {ev:?} -> {:?}", f.name, rule.emit);
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
    features: &[FeatureInfo<D>],
    elsewhere: &[&[D::EventKind]],
) -> Vec<D::EventKind> {
    D::all_kinds()
        .iter()
        .copied()
        .filter(|k| !features.iter().any(|f| f.handles().contains(k)))
        .filter(|k| !elsewhere.iter().any(|ks| ks.contains(k)))
        .collect()
}

/// Actions no feature emits — dead unless the machine layer runs them.
///
/// - `features`: the controller's feature list.
///
/// Returns every action not listed in any feature's [`FeatureInfo::emits`].
pub fn unemitted_actions<D: Domain>(features: &[FeatureInfo<D>]) -> Vec<D::Action>
where
    D::Action: Enumerable,
{
    <D::Action as Enumerable>::ALL
        .iter()
        .copied()
        .filter(|a| !features.iter().any(|f| f.emits().contains(a)))
        .collect()
}
