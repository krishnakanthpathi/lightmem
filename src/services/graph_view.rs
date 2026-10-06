use crate::models::{GraphEdge, GraphNode, GraphSnapshot};
use colored::Colorize;
use std::collections::{HashMap, HashSet};

pub fn render_terminal(snapshot: &GraphSnapshot) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "\n  {} {}\n",
        "❖".bold().bright_magenta(),
        "L I G H T M E M   K N O W L E D G E   G R A P H".bold()
    ));
    out.push_str(&format!(
        "  {}\n",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
    ));
    out.push_str(&format!(
        "  {} {} active memories  ·  {} direct relations\n\n",
        "◈".bright_cyan(),
        snapshot.nodes.len().to_string().bold(),
        snapshot.edges.len().to_string().bold()
    ));

    if snapshot.nodes.is_empty() {
        out.push_str("  No memories or connections found.\n\n");
        return out;
    }

    let mut edges_by_src: HashMap<String, Vec<&GraphEdge>> = HashMap::new();
    for edge in &snapshot.edges {
        edges_by_src
            .entry(edge.source.clone())
            .or_default()
            .push(edge);
    }

    let node_map: HashMap<String, &GraphNode> =
        snapshot.nodes.iter().map(|n| (n.id.clone(), n)).collect();

    // Sort nodes: hubs with highest degree first
    let mut sorted_nodes = snapshot.nodes.clone();
    sorted_nodes.sort_by(|a, b| b.degree.cmp(&a.degree));

    let mut visited_roots = HashSet::new();

    for node in &sorted_nodes {
        if node.degree == 0 && visited_roots.len() > 10 {
            continue; // Truncate long list of isolated nodes in terminal
        }
        if visited_roots.contains(&node.id) {
            continue;
        }
        visited_roots.insert(node.id.clone());

        let short_id = if node.id.len() >= 8 {
            &node.id[..8]
        } else {
            &node.id
        };
        out.push_str(&format!(
            "  {} [{}] {} ({})\n",
            "◈".bright_blue(),
            node.category.to_lowercase().bright_yellow(),
            node.label.bold(),
            short_id.bright_black()
        ));

        if let Some(out_edges) = edges_by_src.get(&node.id) {
            for (idx, edge) in out_edges.iter().enumerate() {
                let is_last = idx == out_edges.len() - 1;
                let branch = if is_last { "└──" } else { "├──" };
                let target_label = node_map
                    .get(&edge.target)
                    .map(|n| format!("[{}] {}", n.category.to_lowercase(), n.label))
                    .unwrap_or_else(|| edge.target.clone());

                out.push_str(&format!(
                    "  │   {} ──{}──▶ {}\n",
                    branch.bright_black(),
                    edge.relation.bright_cyan(),
                    target_label
                ));
            }
        }
        out.push('\n');
    }

    out
}
