//! Phase 2 Story 14.2 — citation graph aggregation.
//!
//! Aggregates per-(provider, source_domain) citation counts into a
//! directed graph: providers point at the domains they cited, with edge
//! weights equal to the citation count.
//!
//! Wire shape: nodes + edges arrays. Nodes carry (`id`, `kind`,
//! `label`); edges carry (`source`, `target`, `weight`). Renderable by
//! `apps/web`'s force-directed layout (D3) and by the architecture-
//! mandated a11y companion table that lists the same edges in
//! tabular form.
//!
//! Pure function — no DB. The caller passes pre-fetched citation rows;
//! the aggregation here is deterministic so Story 14.1's parity test
//! against ClickHouse can byte-compare.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One pre-aggregated citation observation. The caller pre-joins
/// citations + prompt_runs + providers and SUMs the `citations.frequency`
/// column into a single weight. A single citation row in the DB with
/// `frequency=5` contributes the same `weight=5` here as five separate
/// rows would — this keeps the citation graph's edge weights consistent
/// with the `citation_summary` surface that already sums `frequency`.
#[derive(Debug, Clone, PartialEq)]
pub struct CitationRow {
    pub provider: String,
    pub domain: String,
    /// Number of citations this (provider, domain) tuple contributes to
    /// the edge weight. Must be ≥ 1.
    pub weight: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub kind: NodeKind,
    /// Operator-friendly label. Equal to `id` for providers; for
    /// domains, equals the domain string (no transformation).
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Provider,
    Domain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    /// `id` of the source node (always a `Provider`).
    pub source: String,
    /// `id` of the target node (always a `Domain`).
    pub target: String,
    /// Number of times this provider cited this domain in the window.
    pub weight: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CitationGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// Build a citation graph from a flat list of observed citations.
/// Edges are deduplicated; multiple citations of the same `(provider,
/// domain)` pair add to the edge's `weight`. Output ordering is
/// deterministic: nodes by `(kind, id)`, edges by `(source, target)`.
pub fn compute(rows: &[CitationRow]) -> CitationGraph {
    let mut edge_weights: BTreeMap<(String, String), u32> = BTreeMap::new();
    for row in rows {
        let w = row.weight.max(1);
        *edge_weights
            .entry((row.provider.clone(), row.domain.clone()))
            .or_insert(0) += w;
    }

    let mut providers: BTreeMap<String, ()> = BTreeMap::new();
    let mut domains: BTreeMap<String, ()> = BTreeMap::new();
    for (provider, domain) in edge_weights.keys() {
        providers.insert(provider.clone(), ());
        domains.insert(domain.clone(), ());
    }

    let mut nodes = Vec::with_capacity(providers.len() + domains.len());
    for id in providers.keys() {
        nodes.push(GraphNode {
            id: id.clone(),
            kind: NodeKind::Provider,
            label: id.clone(),
        });
    }
    for id in domains.keys() {
        nodes.push(GraphNode {
            id: id.clone(),
            kind: NodeKind::Domain,
            label: id.clone(),
        });
    }

    let edges = edge_weights
        .into_iter()
        .map(|((source, target), weight)| GraphEdge {
            source,
            target,
            weight,
        })
        .collect();

    CitationGraph { nodes, edges }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(provider: &str, domain: &str) -> CitationRow {
        CitationRow {
            provider: provider.to_string(),
            domain: domain.to_string(),
            weight: 1,
        }
    }

    #[test]
    fn empty_input_returns_empty_graph() {
        let g = compute(&[]);
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
    }

    #[test]
    fn one_citation_yields_two_nodes_and_one_edge() {
        let g = compute(&[row("openai", "docs.example.com")]);
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].weight, 1);
    }

    #[test]
    fn duplicate_pairs_sum_into_edge_weight() {
        let g = compute(&[
            row("openai", "docs.example.com"),
            row("openai", "docs.example.com"),
            row("openai", "docs.example.com"),
        ]);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].weight, 3);
    }

    #[test]
    fn providers_listed_before_domains() {
        // Stable rendering: the dashboard's force-directed layout
        // anchors providers on the left. Ordering helps the layout
        // converge deterministically across runs.
        let g = compute(&[
            row("openai", "docs.example.com"),
            row("anthropic", "wikipedia.org"),
        ]);
        let kinds: Vec<NodeKind> = g.nodes.iter().map(|n| n.kind).collect();
        assert_eq!(
            kinds,
            vec![
                NodeKind::Provider,
                NodeKind::Provider,
                NodeKind::Domain,
                NodeKind::Domain,
            ]
        );
    }

    #[test]
    fn node_order_is_lex_within_kind() {
        let g = compute(&[
            row("openai", "zeta.com"),
            row("anthropic", "alpha.com"),
            row("openai", "mu.com"),
        ]);
        let provider_ids: Vec<&str> = g
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Provider)
            .map(|n| n.id.as_str())
            .collect();
        let domain_ids: Vec<&str> = g
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Domain)
            .map(|n| n.id.as_str())
            .collect();
        assert_eq!(provider_ids, vec!["anthropic", "openai"]);
        assert_eq!(domain_ids, vec!["alpha.com", "mu.com", "zeta.com"]);
    }

    #[test]
    fn edges_ordered_by_source_then_target() {
        let g = compute(&[
            row("openai", "z.com"),
            row("anthropic", "a.com"),
            row("openai", "a.com"),
            row("anthropic", "z.com"),
        ]);
        let pairs: Vec<(&str, &str)> = g
            .edges
            .iter()
            .map(|e| (e.source.as_str(), e.target.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("anthropic", "a.com"),
                ("anthropic", "z.com"),
                ("openai", "a.com"),
                ("openai", "z.com"),
            ]
        );
    }

    #[test]
    fn graph_is_byte_stable_for_arch_26a_parity() {
        let rows = vec![
            row("openai", "docs.example.com"),
            row("openai", "wikipedia.org"),
            row("anthropic", "docs.example.com"),
            row("openai", "wikipedia.org"),
        ];
        let g1 = compute(&rows);
        let g2 = compute(&rows);
        assert_eq!(
            serde_json::to_vec(&g1).unwrap(),
            serde_json::to_vec(&g2).unwrap()
        );
    }

    #[test]
    fn node_label_equals_id_for_provider_and_domain() {
        let g = compute(&[row("openai", "docs.example.com")]);
        for n in &g.nodes {
            assert_eq!(n.id, n.label);
        }
    }

    #[test]
    fn round_trip_through_serde_preserves_shape() {
        let g = compute(&[
            row("openai", "docs.example.com"),
            row("anthropic", "wikipedia.org"),
        ]);
        let bytes = serde_json::to_vec(&g).unwrap();
        let back: CitationGraph = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, g);
    }

    #[test]
    fn node_kind_serializes_as_snake_case() {
        // Wire-shape pin: the dashboard checks `kind === "provider"`
        // vs `"domain"`. Any rename refactor must surface here.
        let g = compute(&[row("openai", "docs.example.com")]);
        let json = serde_json::to_value(&g).unwrap();
        assert_eq!(json["nodes"][0]["kind"], "provider");
        assert_eq!(json["nodes"][1]["kind"], "domain");
    }
}
