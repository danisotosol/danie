//! The learning plan: a directed acyclic graph of topics with prerequisites.

use std::collections::HashMap;

use petgraph::algo::{is_cyclic_directed, toposort};
use petgraph::prelude::*;
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};

use crate::{CoreError, Result};

/// One topic in the learning plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanNode {
    /// Slug-like stable identifier, e.g. "variables".
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// One-line summary of what the node covers.
    pub summary: String,
}

/// A prerequisite DAG over plan nodes.
#[derive(Default)]
pub struct PlanGraph {
    graph: DiGraph<PlanNode, ()>,
    ids: HashMap<String, NodeIndex>,
}

/// JSON shape used by [`PlanGraph::to_json`] / [`PlanGraph::from_json`].
#[derive(Debug, Serialize, Deserialize)]
struct PlanData {
    nodes: Vec<PlanNode>,
    edges: Vec<[String; 2]>,
}

impl PlanGraph {
    /// Creates an empty plan graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a node keyed by its unique `id`; duplicate ids are rejected.
    pub fn add_node(&mut self, node: PlanNode) -> Result<()> {
        if self.ids.contains_key(&node.id) {
            return Err(CoreError::InvalidFormat(format!(
                "duplicate node id in plan: {}",
                node.id
            )));
        }
        let idx = self.graph.add_node(node);
        self.ids.insert(self.graph[idx].id.clone(), idx);
        Ok(())
    }

    /// Declares that `before_id` must be learned before `after_id`.
    ///
    /// Rejects unknown ids and edges that would introduce a cycle (the edge is
    /// rolled back in that case).
    pub fn add_prereq(&mut self, before_id: &str, after_id: &str) -> Result<()> {
        let before = *self
            .ids
            .get(before_id)
            .ok_or_else(|| CoreError::NotFound(before_id.to_string()))?;
        let after = *self
            .ids
            .get(after_id)
            .ok_or_else(|| CoreError::NotFound(after_id.to_string()))?;
        let edge = self.graph.add_edge(before, after, ());
        if is_cyclic_directed(&self.graph) {
            self.graph.remove_edge(edge);
            return Err(CoreError::Cycle);
        }
        Ok(())
    }

    /// Number of nodes in the plan.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Returns all nodes in insertion order.
    pub fn nodes(&self) -> Vec<&PlanNode> {
        self.graph.node_indices().map(|i| &self.graph[i]).collect()
    }

    /// Returns all prerequisite edges as `(before_id, after_id)` pairs.
    pub fn edges(&self) -> Vec<(String, String)> {
        self.graph
            .edge_references()
            .map(|e| {
                (
                    self.graph[e.source()].id.clone(),
                    self.graph[e.target()].id.clone(),
                )
            })
            .collect()
    }

    /// Returns the nodes in topological order (prerequisites first).
    pub fn topo_order(&self) -> Result<Vec<&PlanNode>> {
        let order = toposort(&self.graph, None).map_err(|_| CoreError::Cycle)?;
        Ok(order.into_iter().map(|i| &self.graph[i]).collect())
    }

    /// Returns nodes with no prerequisites.
    pub fn roots(&self) -> Vec<&PlanNode> {
        self.graph
            .node_indices()
            .filter(|i| {
                self.graph
                    .neighbors_directed(*i, Direction::Incoming)
                    .next()
                    .is_none()
            })
            .map(|i| &self.graph[i])
            .collect()
    }

    /// Picks the next node to teach: scanning in topological order, the first
    /// node whose prerequisites are all in `known_ids` and whose own id is not
    /// known yet.
    pub fn next_unlocked(
        &self,
        known_ids: &std::collections::HashSet<String>,
    ) -> Option<&PlanNode> {
        for node in self.topo_order().ok()? {
            let idx = self.ids[&node.id];
            let prereqs_satisfied = self
                .graph
                .neighbors_directed(idx, Direction::Incoming)
                .all(|p| known_ids.contains(&self.graph[p].id));
            if prereqs_satisfied && !known_ids.contains(&node.id) {
                return Some(node);
            }
        }
        None
    }

    /// Renders the plan as a Mermaid flowchart with sanitized titles.
    pub fn to_mermaid(&self) -> String {
        let mut out = String::from("flowchart TD\n");
        for i in self.graph.node_indices() {
            let node = &self.graph[i];
            out.push_str(&format!(
                "    {}[\"{}\"]\n",
                node.id,
                node.title.replace('"', "'")
            ));
        }
        for edge in self.graph.edge_references() {
            out.push_str(&format!(
                "    {} --> {}\n",
                self.graph[edge.source()].id,
                self.graph[edge.target()].id
            ));
        }
        out
    }

    /// Serializes the graph (nodes in insertion order plus prerequisite edge
    /// pairs `[before_id, after_id]`) as pretty JSON.
    pub fn to_json(&self) -> Result<String> {
        let nodes: Vec<PlanNode> = self
            .graph
            .node_indices()
            .map(|i| self.graph[i].clone())
            .collect();
        let edges: Vec<[String; 2]> = self
            .graph
            .edge_references()
            .map(|e| {
                [
                    self.graph[e.source()].id.clone(),
                    self.graph[e.target()].id.clone(),
                ]
            })
            .collect();
        serde_json::to_string_pretty(&PlanData { nodes, edges }).map_err(CoreError::from)
    }

    /// Rebuilds a graph from JSON produced by [`PlanGraph::to_json`].
    ///
    /// All structural rules apply during reconstruction: duplicate ids,
    /// unknown prereq references and cycles are rejected.
    pub fn from_json(text: &str) -> Result<Self> {
        let data: PlanData = serde_json::from_str(text)?;
        let mut graph = PlanGraph::new();
        for node in data.nodes {
            graph.add_node(node)?;
        }
        for [before, after] in data.edges {
            graph.add_prereq(&before, &after)?;
        }
        Ok(graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_graph() -> PlanGraph {
        let mut g = PlanGraph::new();
        for (id, title) in [
            ("variables", "Variables"),
            ("tipos", "Tipos"),
            ("funciones", "Funciones"),
            ("closures", "Closures"),
        ] {
            g.add_node(PlanNode {
                id: id.into(),
                title: title.into(),
                summary: format!("Resumen de {id}"),
            })
            .unwrap();
        }
        g.add_prereq("variables", "funciones").unwrap();
        g.add_prereq("funciones", "tipos").unwrap();
        g.add_prereq("tipos", "closures").unwrap();
        g
    }

    fn node_ids<'a>(nodes: &[&'a PlanNode]) -> Vec<&'a str> {
        nodes.iter().map(|n| n.id.as_str()).collect()
    }

    #[test]
    fn duplicate_node_id_is_invalid_format() {
        let mut g = PlanGraph::new();
        let node = || PlanNode {
            id: "variables".into(),
            title: "Variables".into(),
            summary: String::new(),
        };
        g.add_node(node()).unwrap();
        assert!(matches!(
            g.add_node(node()),
            Err(CoreError::InvalidFormat(_))
        ));
    }

    #[test]
    fn unknown_prereq_is_not_found() {
        let mut g = PlanGraph::new();
        g.add_node(PlanNode {
            id: "variables".into(),
            title: "Variables".into(),
            summary: String::new(),
        })
        .unwrap();
        assert!(matches!(
            g.add_prereq("fantasma", "variables"),
            Err(CoreError::NotFound(_))
        ));
        assert!(matches!(
            g.add_prereq("variables", "fantasma"),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn cycle_detection_rolls_back_the_edge() {
        let mut g = sample_graph();
        assert!(g.add_prereq("closures", "variables").is_err());
        assert!(matches!(
            g.add_prereq("closures", "variables"),
            Err(CoreError::Cycle)
        ));
        assert_eq!(g.node_count(), 4);
        assert!(g.topo_order().is_ok());
    }

    #[test]
    fn topo_order_lists_prerequisites_first_and_roots_are_sources() {
        let g = sample_graph();
        let order = node_ids(&g.topo_order().unwrap());
        let pos = |id: &str| order.iter().position(|x| *x == id).unwrap();
        assert!(pos("variables") < pos("funciones"));
        assert!(pos("funciones") < pos("tipos"));
        assert!(pos("tipos") < pos("closures"));
        assert_eq!(node_ids(&g.roots()), vec!["variables"]);
    }

    #[test]
    fn next_unlocked_skips_locked_nodes_in_topological_order() {
        let g = sample_graph();
        let known: std::collections::HashSet<String> =
            ["variables"].iter().map(|s| s.to_string()).collect();
        let next = g.next_unlocked(&known).unwrap();
        assert_eq!(next.id, "funciones");

        let more: std::collections::HashSet<String> = ["variables", "funciones"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(g.next_unlocked(&more).unwrap().id, "tipos");

        let all: std::collections::HashSet<String> =
            ["variables", "tipos", "funciones", "closures"]
                .iter()
                .map(|s| s.to_string())
                .collect();
        assert!(g.next_unlocked(&all).is_none());
    }

    #[test]
    fn mermaid_contains_node_decls_and_edges() {
        let g = sample_graph();
        let mermaid = g.to_mermaid();
        assert!(mermaid.starts_with("flowchart TD\n"));
        assert!(mermaid.contains("variables[\"Variables\"]"));
        assert!(mermaid.contains("tipos[\"Tipos\"]"));
        assert!(mermaid.contains("variables --> funciones"));
        assert!(mermaid.contains("funciones --> tipos"));
        assert!(mermaid.contains("tipos --> closures"));
    }

    #[test]
    fn mermaid_sanitizes_quotes_in_titles() {
        let mut g = PlanGraph::new();
        g.add_node(PlanNode {
            id: "comillas".into(),
            title: "El arte de \"decir\"".into(),
            summary: String::new(),
        })
        .unwrap();
        let mermaid = g.to_mermaid();
        assert!(!mermaid.contains("\"decir\"\""));
        assert!(mermaid.contains("comillas[\"El arte de 'decir'\"]"));
    }

    #[test]
    fn json_roundtrip_preserves_structure() {
        let g = sample_graph();
        let json = g.to_json().unwrap();
        let back = PlanGraph::from_json(&json).unwrap();
        assert_eq!(back.node_count(), g.node_count());
        assert_eq!(
            back.to_mermaid(),
            "flowchart TD\n    variables[\"Variables\"]\n    tipos[\"Tipos\"]\n    funciones[\"Funciones\"]\n    closures[\"Closures\"]\n    variables --> funciones\n    funciones --> tipos\n    tipos --> closures\n"
        );
        let known: std::collections::HashSet<String> =
            ["variables"].iter().map(|s| s.to_string()).collect();
        assert_eq!(back.next_unlocked(&known).unwrap().id, "funciones");
    }

    #[test]
    fn from_json_rejects_unknown_edge_references() {
        let json = r#"{"nodes":[{"id":"a","title":"A","summary":""}],"edges":[["ghost","a"]]}"#;
        assert!(matches!(
            PlanGraph::from_json(json),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn from_json_rejects_cycles() {
        let json = r#"{"nodes":[{"id":"a","title":"A","summary":""},{"id":"b","title":"B","summary":""}],"edges":[["a","b"],["b","a"]]}"#;
        assert!(matches!(PlanGraph::from_json(json), Err(CoreError::Cycle)));
    }
}
