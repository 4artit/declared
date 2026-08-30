//! The stateless layer: one feature of a controller, declaring what it reacts to
//! and what it does about it.

use crate::{Domain, Enumerable, HasKind};

/// What a feature reacts to and what it emits.
///
/// A plain value, so features can be collected into a slice for
/// [`crate::render::io_table`].
pub struct FeatureInfo<D: Domain> {
    /// Display name, used in tables and diagrams.
    pub name: &'static str,
    /// Event kinds this feature reacts to.
    pub handles: &'static [D::EventKind],
    /// Actions this feature may emit.
    pub emits: &'static [D::Action],
}

/// One feature of a controller: what it reacts to and what it emits.
pub trait Feature<D: Domain> {
    /// This feature's declared inputs and outputs.
    const INFO: FeatureInfo<D>;

    /// Reacts to `ev` by pushing effects onto `out`. [`dispatch`] checks each
    /// one against [`FeatureInfo::emits`] before [`Domain::perform`] runs it.
    ///
    /// A feature decides, it does not act: `world` is read-only here so that
    /// [`FeatureInfo::emits`] is the whole truth about what this feature can
    /// do, which is what the tables and diagrams are drawn from.
    ///
    /// - `ev`: the event to react to.
    /// - `world`: the outside world, read-only.
    /// - `out`: effects to append, in the order they should run.
    fn handle(&mut self, ev: &D::Event, world: &D::Env, out: &mut Vec<D::Action>);
}

/// Runs `f` on `ev` if `f` declared this event's kind, checks that it only
/// emitted actions it declared, then carries those actions out.
///
/// - `f`: the feature to run.
/// - `ev`: the event to dispatch.
/// - `world`: the outside world, read by `f` and mutated by its actions.
///
/// The action list is this function's own, so a feature's effects land in
/// `world` before the next feature is dispatched. Two features that declare the
/// same kind in [`FeatureInfo::handles`] therefore see each other, in the order
/// the caller dispatches them.
///
/// # Panics
///
/// In debug builds, panics if `f` emitted an action outside
/// [`FeatureInfo::emits`] — before any of them runs.
pub fn dispatch<D, F>(f: &mut F, ev: &D::Event, world: &mut D::Env)
where
    D: Domain,
    D::Action: PartialEq,
    F: Feature<D>,
{
    if !F::INFO.handles.contains(&ev.kind()) {
        return;
    }

    // Unallocated until the handler pushes, which the gate above already ruled
    // out for most events.
    let mut out = Vec::new();
    f.handle(ev, &*world, &mut out);

    debug_assert!(
        out.iter().all(|a| F::INFO.emits.contains(a)),
        "{}: emitted an action it does not declare -> {out:?}",
        F::INFO.name,
    );

    for a in out {
        D::perform(a, ev, world);
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
    features: &[FeatureInfo<D>],
    elsewhere: &[&[D::EventKind]],
) -> Vec<D::EventKind> {
    D::all_kinds()
        .iter()
        .copied()
        .filter(|k| !features.iter().any(|f| f.handles.contains(k)))
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
        .filter(|a| !features.iter().any(|f| f.emits.contains(a)))
        .collect()
}
