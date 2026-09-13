//! Exhaustive checks over a controller's declaration: the combinations a
//! transition table leaves out, the declarations it contradicts, and the guard
//! names it reuses across either layer.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::any::TypeId;

use crate::feature::{AnyFeature, Feature};
use crate::machine::{Edge, Goto, Ignore};
use crate::{Domain, Enumerable, MachineSpec};

/// The result of checking every `(state × event kind)` combination.
#[derive(Debug, Default)]
pub struct Coverage {
    /// The machine this reports on, from [`MachineSpec::NAME`]. A controller
    /// asserting several of these needs the failure to say which one.
    pub machine: &'static str,
    /// Combinations with neither an edge nor an [`Ignore`]. **Must be empty in
    /// CI.**
    pub holes: Vec<(String, String)>,
    /// Combinations an [`Ignore`] forbids and an edge handles anyway, with the
    /// offending edge ids.
    pub ignored_but_handled: Vec<(String, String, Vec<&'static str>)>,
    /// Combinations matched by more than one edge. Not necessarily wrong, since
    /// declaration order is priority, but worth reviewing.
    pub overlaps: Vec<(String, String, Vec<&'static str>)>,
    /// States that cannot be reached from the initial state.
    pub unreachable: Vec<String>,
    /// Guard node names used by more than one node type. The [`crate::guard::Memo`] key
    /// is the name, so names must be unique.
    pub duplicate_node_names: Vec<&'static str>,
    /// Edge ids carried by more than one edge. The id names a transition in
    /// [`crate::machine::Taken`], in logs and in golden diffs.
    pub duplicate_edge_ids: Vec<&'static str>,
}

impl Coverage {
    /// Whether the table is free of defects. `overlaps` is excluded: it is a
    /// review signal, not an error.
    pub fn is_clean(&self) -> bool {
        self.holes.is_empty()
            && self.ignored_but_handled.is_empty()
            && self.unreachable.is_empty()
            && self.duplicate_node_names.is_empty()
            && self.duplicate_edge_ids.is_empty()
    }
}

/// Checks a transition table for gaps, contradicted [`Ignore`]s, unreachable
/// states, and duplicate names.
///
/// - `initial`: the machine's starting state.
/// - `edges`, `ignores`: the transition table to check.
///
/// Returns a [`Coverage`] report.
pub fn coverage<M: MachineSpec>(
    initial: M::Tag,
    edges: &'static [Edge<M>],
    ignores: &'static [Ignore<M>],
) -> Coverage {
    let mut out = Coverage {
        machine: M::NAME,
        ..Default::default()
    };

    for &tag in M::all_tags() {
        for &kind in <M::Domain as Domain>::all_kinds() {
            let hits: Vec<&'static str> = edges
                .iter()
                .filter(|e| e.when == kind && e.from.matches(tag))
                .map(|e| e.id)
                .collect();

            // An `Ignore` is a floor, not a fallback: guards do not enter into
            // it, so a single edge on the combination contradicts the ban.
            let ignored = ignores.iter().any(|i| i.matches(tag, kind));

            if hits.is_empty() {
                if !ignored {
                    out.holes.push((format!("{tag:?}"), format!("{kind:?}")));
                }
            } else {
                if ignored {
                    out.ignored_but_handled.push((
                        format!("{tag:?}"),
                        format!("{kind:?}"),
                        hits.clone(),
                    ));
                }
                if hits.len() > 1 {
                    out.overlaps
                        .push((format!("{tag:?}"), format!("{kind:?}"), hits));
                }
            }
        }
    }

    // Tag is only required to be Copy + Eq + Debug, not Hash, so reachability uses
    // a linear scan. State counts are small, and this avoids treating two states
    // with coincidentally equal Debug output as the same state.
    let mut reached: Vec<M::Tag> = vec![initial];
    // Iterate to a fixed point; tables are small enough that this is fine.
    loop {
        let before = reached.len();
        for e in edges {
            if let Goto::To(next) = e.goto
                && !reached.contains(&next)
                && e.from.expand().iter().any(|t| reached.contains(t))
            {
                reached.push(next);
            }
        }
        if reached.len() == before {
            break;
        }
    }
    for &tag in M::all_tags() {
        if !reached.contains(&tag) {
            out.unreachable.push(format!("{tag:?}"));
        }
    }

    // Only this machine's own edges: a guard shared with a feature is checked
    // controller-wide by `duplicate_node_names`, which this cannot see.
    out.duplicate_node_names = names_shared_by_two_types(guard_nodes(edges));

    // Unlike node names, every repeat is a defect, so the list is not deduped
    // before the scan.
    let mut edge_ids: Vec<&'static str> = edges.iter().map(|e| e.id).collect();
    edge_ids.sort_unstable();
    for w in edge_ids.windows(2) {
        if w[0] == w[1] && !out.duplicate_edge_ids.contains(&w[0]) {
            out.duplicate_edge_ids.push(w[0]);
        }
    }

    out
}

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

/// The guard nodes a transition table references, as `(name, type id)`.
///
/// Pass this to [`duplicate_node_names`] alongside a feature list, the way
/// [`handled_kinds`] is passed to [`unhandled_kinds`], so a
/// controller mixing both layers is checked as one unit.
///
/// - `edges`: the transition table to scan.
///
/// Returns one pair per node reference, repeats included — [`duplicate_node_names`]
/// dedupes.
pub fn guard_nodes<M: MachineSpec>(edges: &'static [Edge<M>]) -> Vec<(&'static str, TypeId)> {
    let mut out: Vec<(&'static str, TypeId)> = Vec::new();
    for e in edges {
        e.check.node_ids(&mut out);
    }
    out
}

/// Guard node names used by more than one node type across a whole controller.
///
/// Node names are unique per domain: [`crate::guard::Memo`] keys on the name, so
/// two node types answering to one name make the second inherit the first's
/// result without running.
///
/// [`Coverage::duplicate_node_names`] is the same check over one machine's
/// edges. This one spans both layers, which no [`Coverage`] can see.
///
/// - `features`: the controller's feature list.
/// - `elsewhere`: nodes referenced outside that list, e.g. [`guard_nodes`] for
///   each state machine the controller also runs; pass `&[]` if there are none.
///
/// Returns the offending names, sorted and without repeats. **Must be empty in
/// CI.**
pub fn duplicate_node_names<D: Domain>(
    features: &[&dyn AnyFeature<D>],
    elsewhere: &[&[(&'static str, TypeId)]],
) -> Vec<&'static str> {
    let mut ids: Vec<(&'static str, TypeId)> = Vec::new();
    for f in features {
        f.node_ids(&mut ids);
    }
    for group in elsewhere {
        ids.extend_from_slice(group);
    }
    names_shared_by_two_types(ids)
}

/// Rule ids carried by more than one rule across a whole controller.
///
/// [`crate::feature::AnyFeature::dispatch`] returns an id with no feature name
/// attached, so a repeat would name two different rules. The machine-side
/// counterpart is [`Coverage::duplicate_edge_ids`].
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

/// The event kinds a transition table acts on. `Ignore`d kinds do not count —
/// pass this to [`unhandled_kinds`] alongside a feature list so
/// a controller mixing both layers is checked as one unit.
///
/// - `edges`: the transition table to scan.
///
/// Returns the event kinds matched by at least one edge.
pub fn handled_kinds<M: MachineSpec>(
    edges: &'static [Edge<M>],
) -> Vec<<M::Domain as Domain>::EventKind> {
    let mut out: Vec<_> = Vec::new();
    for e in edges {
        if !out.contains(&e.when) {
            out.push(e.when);
        }
    }
    out
}

/// Event kinds nothing in the controller accounts for.
///
/// - `features`: the controller's feature list.
/// - `elsewhere`: kinds handled outside that list, e.g.
///   [`handled_kinds`] for each state machine the controller also
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
