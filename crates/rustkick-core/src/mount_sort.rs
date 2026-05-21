//! Generic Kahn topo-sort used by plugins, adapters, and contributors.
//!
//! Mirrors KickJS `mountSort`. Boot fails with structured errors:
//! `RK_E_DUPLICATE_MOUNT`, `RK_E_MISSING_MOUNT_DEP`, `RK_E_MOUNT_CYCLE`.

use crate::error::{KickError, KickResult};
use std::collections::{HashMap, VecDeque};

/// Anything sortable: a stable name and a list of names it depends on.
pub trait MountItem {
    /// Stable identifier.
    fn name(&self) -> &str;
    /// Names of items that must come earlier.
    fn depends_on(&self) -> &[&str];
}

/// Sort `items` topologically by `depends_on`. Returns the reordered
/// list, or a structured `KickError` describing the graph problem.
pub fn topo_sort<T: MountItem>(items: Vec<T>) -> KickResult<Vec<T>> {
    // Pre-collect owned names so the borrow checker doesn't fight us
    // when we later move out of `items` to reorder.
    let names: Vec<String> = items.iter().map(|i| i.name().to_owned()).collect();
    let deps: Vec<Vec<String>> = items
        .iter()
        .map(|i| i.depends_on().iter().map(|d| (*d).to_owned()).collect())
        .collect();

    // Validate uniqueness.
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (idx, n) in names.iter().enumerate() {
        if let Some(prev) = seen.insert(n.as_str(), idx) {
            return Err(KickError::new(
                "RK_E_DUPLICATE_MOUNT",
                format!("duplicate mount name `{}`", n),
            )
            .with_hint("rename or `.scoped(name)` one of them")
            .with_context("duplicate_of", &names[prev]));
        }
    }

    // Build in-degree map + edges, all using owned strings.
    let mut in_degree: HashMap<String, usize> =
        names.iter().map(|n| (n.clone(), 0usize)).collect();
    let mut edges: HashMap<String, Vec<String>> = HashMap::new();
    for (i, n) in names.iter().enumerate() {
        for dep in &deps[i] {
            if !in_degree.contains_key(dep) {
                return Err(KickError::new(
                    "RK_E_MISSING_MOUNT_DEP",
                    format!("`{}` depends on unknown mount `{}`", n, dep),
                )
                .with_hint("add the missing item, or remove the dependency"));
            }
            edges.entry(dep.clone()).or_default().push(n.clone());
            *in_degree.get_mut(n).unwrap() += 1;
        }
    }

    // Kahn.
    let mut ready: VecDeque<String> = in_degree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(n, _)| n.clone())
        .collect();
    let mut order: Vec<String> = Vec::with_capacity(names.len());
    while let Some(n) = ready.pop_front() {
        if let Some(outs) = edges.get(&n) {
            // clone to release the borrow before mutating in_degree
            let outs = outs.clone();
            for next in outs {
                let d = in_degree.get_mut(&next).expect("dep in graph");
                *d -= 1;
                if *d == 0 {
                    ready.push_back(next);
                }
            }
        }
        order.push(n);
    }
    if order.len() != names.len() {
        return Err(KickError::new("RK_E_MOUNT_CYCLE", "cycle detected in mount graph")
            .with_hint("break the cycle in `depends_on` declarations"));
    }

    // Reorder original items by computed order.
    let order_index: HashMap<String, usize> =
        order.into_iter().enumerate().map(|(i, n)| (n, i)).collect();
    let mut indexed: Vec<(usize, T)> = items
        .into_iter()
        .map(|item| {
            let idx = order_index[item.name()];
            (idx, item)
        })
        .collect();
    indexed.sort_by_key(|(i, _)| *i);
    Ok(indexed.into_iter().map(|(_, x)| x).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Item {
        n: &'static str,
        deps: &'static [&'static str],
    }
    impl MountItem for Item {
        fn name(&self) -> &str {
            self.n
        }
        fn depends_on(&self) -> &[&str] {
            self.deps
        }
    }

    #[test]
    fn sorts_linear_chain() {
        let items = vec![
            Item { n: "c", deps: &["b"] },
            Item { n: "a", deps: &[] },
            Item { n: "b", deps: &["a"] },
        ];
        let sorted = topo_sort(items).unwrap();
        let names: Vec<_> = sorted.iter().map(|i| i.n).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn detects_cycle() {
        let items = vec![
            Item { n: "a", deps: &["b"] },
            Item { n: "b", deps: &["a"] },
        ];
        let err = topo_sort(items).unwrap_err();
        assert_eq!(err.code, "RK_E_MOUNT_CYCLE");
    }

    #[test]
    fn detects_missing_dep() {
        let items = vec![Item { n: "a", deps: &["ghost"] }];
        let err = topo_sort(items).unwrap_err();
        assert_eq!(err.code, "RK_E_MISSING_MOUNT_DEP");
    }

    #[test]
    fn detects_duplicate() {
        let items = vec![
            Item { n: "a", deps: &[] },
            Item { n: "a", deps: &[] },
        ];
        let err = topo_sort(items).unwrap_err();
        assert_eq!(err.code, "RK_E_DUPLICATE_MOUNT");
    }
}
