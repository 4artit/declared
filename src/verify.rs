//! Reports over a controller's declaration: the events nothing accounts for,
//! the effects nothing produces, and the names it reuses.

use alloc::vec::Vec;
use core::any::TypeId;

use crate::feature::{AnyFeature, Feature};
use crate::{Domain, Enumerable};

/// Names carried by more than one node *type*.
///
/// Deduping first is what makes reuse legal: one node named on twenty rows
/// collapses to a single pair, so only a name two types answer to survives.
fn names_shared_by_two_types(mut node_ids: Vec<(&'static str, TypeId)>) -> Vec<&'static str> {
    node_ids.sort_unstable();
    node_ids.dedup();

    let mut out: Vec<&'static str> = Vec::new();
    for w in node_ids.windows(2) {
        if w[0].0 == w[1].0 && !out.contains(&w[0].0) {
            out.push(w[0].0);
        }
    }
    out
}

/// Guard node names used by more than one node type across a whole controller.
///
/// Node names are unique per domain: [`crate::guard::Memo`] keys on the name, so
/// two node types answering to one name make the second inherit the first's
/// result without running.
///
/// - `features`: the controller's feature list.
///
/// Returns the offending names, sorted and without repeats. **Must be empty in
/// CI.**
pub fn duplicate_node_names<D: Domain>(features: &[&dyn AnyFeature<D>]) -> Vec<&'static str> {
    let mut ids: Vec<(&'static str, TypeId)> = Vec::new();
    for f in features {
        f.node_ids(&mut ids);
    }
    names_shared_by_two_types(ids)
}

/// Rule ids carried by more than one rule across a whole controller.
///
/// [`crate::feature::AnyFeature::dispatch`] returns an id with no feature name
/// attached, so a repeat would name two different rules.
///
/// - `features`: the controller's feature list.
///
/// Returns the offending ids, sorted and without repeats. **Must be empty in
/// CI.**
pub fn duplicate_rule_ids<D: Domain>(features: &[&dyn AnyFeature<D>]) -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = features
        .iter()
        .flat_map(|f| f.rows())
        .map(|r| r.id)
        .collect();
    ids.sort_unstable();

    let mut out: Vec<&'static str> = Vec::new();
    for w in ids.windows(2) {
        if w[0] == w[1] && !out.contains(&w[0]) {
            out.push(w[0]);
        }
    }
    out
}

/// Event kinds no rule accounts for.
///
/// Not every one of these is a defect: a signal the controller absorbs into its
/// world without deciding anything on it is reported here too, and saying so is
/// the point — an event nobody handles should be a line somebody read, not a
/// silence.
///
/// - `features`: the controller's feature list.
///
/// Returns the event kinds no feature reacts to.
pub fn unhandled_kinds<D: Domain>(features: &[&dyn AnyFeature<D>]) -> Vec<D::EventKind> {
    D::all_kinds()
        .iter()
        .copied()
        .filter(|k| !features.iter().any(|f| f.handles().contains(k)))
        .collect()
}

/// Actions `F` declares but no rule of `F` emits — dead effects.
///
/// One feature at a time rather than a whole controller at once: with an action
/// type per feature, an orphaned effect is a question about the file that owns
/// it.
///
/// Returns every value of `F::Action` missing from every [`crate::feature::Rule::emit`].
pub fn unemitted_actions<F: Feature>() -> Vec<F::Action>
where
    F::Action: Enumerable,
{
    <F::Action as Enumerable>::ALL
        .iter()
        .copied()
        .filter(|a| !F::RULES.iter().any(|r| r.emit.contains(a)))
        .collect()
}
