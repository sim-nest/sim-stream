//! ASCII diagram topology parsing.

use std::collections::{BTreeMap, BTreeSet};

use sim_kernel::{Cx, Error, Expr, Result};

use crate::{
    Graph, TopologyConnection, connection_from_graph,
    text::{graph_from_text, graph_to_expr},
};

/// Parses an ASCII topology diagram and returns canonical graph data.
pub fn parse_diagram(source: &str) -> Result<Expr> {
    graph_from_diagram(source).map(|graph| graph_to_expr(&graph))
}

/// Alias used by the runtime registry for the diagram operation.
pub fn draw(source: &str) -> Result<Expr> {
    parse_diagram(source)
}

/// Parses an ASCII topology diagram directly into a graph.
pub fn graph_from_diagram(source: &str) -> Result<Graph> {
    let text = diagram_to_text(source)?;
    graph_from_text(&text)
}

/// Parses an ASCII topology diagram into a runnable topology connection.
pub fn from_diagram(cx: &mut Cx, source: &str) -> Result<TopologyConnection> {
    let graph = graph_from_diagram(source)?;
    connection_from_graph(cx, &graph)
}

#[derive(Clone, Debug)]
struct DiagramNode {
    id: String,
    verb: String,
    attrs: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
struct DiagramEdge {
    from: String,
    to: String,
    attrs: Vec<(String, String)>,
}

#[derive(Default)]
struct DiagramDraft {
    nodes: Vec<DiagramNode>,
    edges: Vec<DiagramEdge>,
    budget_attrs: Vec<(String, String)>,
}

type SourceLine<'a> = (usize, &'a str);

#[derive(Clone)]
struct BoxRef {
    id: String,
    verb: String,
    start: usize,
    end: usize,
    column: usize,
}

fn diagram_to_text(source: &str) -> Result<String> {
    let (diagram_lines, attrs_lines) = split_sections(source);
    let (line_no, line) = single_box_line(&diagram_lines)?;
    let boxes = parse_boxes(line_no, line)?;
    let mut draft = draft_from_boxes(line_no, line, boxes, &diagram_lines)?;
    apply_attrs(&mut draft, &attrs_lines)?;
    Ok(render_text(&draft))
}

fn split_sections(source: &str) -> (Vec<SourceLine<'_>>, Vec<SourceLine<'_>>) {
    let mut diagram = Vec::new();
    let mut attrs = Vec::new();
    let mut in_attrs = false;
    for (index, line) in source.lines().enumerate() {
        let line_no = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed == "attrs:" {
            in_attrs = true;
            continue;
        }
        if in_attrs {
            attrs.push((line_no, line));
        } else {
            diagram.push((line_no, line));
        }
    }
    (diagram, attrs)
}

fn single_box_line<'a>(lines: &'a [SourceLine<'a>]) -> Result<SourceLine<'a>> {
    let mut matches = lines.iter().copied().filter(|(_, line)| line.contains('['));
    let Some(first) = matches.next() else {
        return Err(diagram_error(1, 1, "missing diagram box line"));
    };
    if let Some((line_no, line)) = matches.next() {
        let column = line.find('[').map(|index| index + 1).unwrap_or(1);
        return Err(diagram_error(
            line_no,
            column,
            "ambiguous diagram: multiple box lines are not supported",
        ));
    }
    Ok(first)
}

fn parse_boxes(line_no: usize, line: &str) -> Result<Vec<BoxRef>> {
    let mut boxes = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = line[offset..].find('[') {
        let start = offset + relative_start;
        let Some(relative_end) = line[start + 1..].find(']') else {
            return Err(diagram_error(line_no, start + 1, "unterminated box"));
        };
        let end = start + 1 + relative_end;
        let content = &line[start + 1..end];
        boxes.push(parse_box(line_no, start + 1, start, end + 1, content)?);
        offset = end + 1;
    }
    if boxes.is_empty() {
        return Err(diagram_error(line_no, 1, "missing boxes"));
    }
    Ok(boxes)
}

fn parse_box(
    line_no: usize,
    column: usize,
    start: usize,
    end: usize,
    content: &str,
) -> Result<BoxRef> {
    if content.is_empty() || content.contains('[') || content.contains(']') {
        return Err(diagram_error(line_no, column, "invalid box"));
    }
    let (id, verb) = match content.split_once(':') {
        Some((id, verb)) if !id.is_empty() && !verb.is_empty() => (id, verb),
        None => (content, content),
        _ => return Err(diagram_error(line_no, column, "expected [id] or [id:verb]")),
    };
    Ok(BoxRef {
        id: id.to_owned(),
        verb: verb.to_owned(),
        start,
        end,
        column,
    })
}

fn draft_from_boxes(
    line_no: usize,
    line: &str,
    boxes: Vec<BoxRef>,
    diagram_lines: &[(usize, &str)],
) -> Result<DiagramDraft> {
    validate_unique_boxes(line_no, &boxes)?;
    validate_horizontal_arrows(line_no, line, &boxes)?;
    let mut draft = DiagramDraft {
        nodes: boxes
            .iter()
            .map(|item| DiagramNode {
                id: item.id.clone(),
                verb: item.verb.clone(),
                attrs: Vec::new(),
            })
            .collect(),
        ..Default::default()
    };
    for pair in boxes.windows(2) {
        let from = if pair[0].verb == "branch" {
            format!("{}:true", pair[0].id)
        } else {
            pair[0].id.clone()
        };
        draft.edges.push(DiagramEdge {
            from,
            to: pair[1].id.clone(),
            attrs: Vec::new(),
        });
    }
    add_back_edge_if_present(&mut draft, diagram_lines)?;
    Ok(draft)
}

fn validate_unique_boxes(line_no: usize, boxes: &[BoxRef]) -> Result<()> {
    let mut ids = BTreeSet::new();
    for item in boxes {
        if !ids.insert(item.id.clone()) {
            return Err(diagram_error(
                line_no,
                item.column,
                format!("duplicate box id {}", item.id),
            ));
        }
    }
    Ok(())
}

fn validate_horizontal_arrows(line_no: usize, line: &str, boxes: &[BoxRef]) -> Result<()> {
    if !line[..boxes[0].start].trim().is_empty() {
        return Err(diagram_error(
            line_no,
            1,
            "unexpected text before first box",
        ));
    }
    for pair in boxes.windows(2) {
        let segment = &line[pair[0].end..pair[1].start];
        if segment.trim() != "-->" {
            return Err(diagram_error(
                line_no,
                pair[0].end + 1,
                "expected --> between boxes",
            ));
        }
    }
    if !line[boxes[boxes.len() - 1].end..].trim().is_empty() {
        return Err(diagram_error(
            line_no,
            boxes[boxes.len() - 1].end + 1,
            "unexpected text after final box",
        ));
    }
    Ok(())
}

fn add_back_edge_if_present(
    draft: &mut DiagramDraft,
    diagram_lines: &[SourceLine<'_>],
) -> Result<()> {
    let Some((line_no, column)) = back_edge_marker(diagram_lines) else {
        return Ok(());
    };
    let branch_indexes = draft
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| (node.verb == "branch").then_some(index))
        .collect::<Vec<_>>();
    match branch_indexes.as_slice() {
        [branch_index] if *branch_index > 0 => {
            let branch = draft.nodes[*branch_index].id.clone();
            let target = draft.nodes[*branch_index - 1].id.clone();
            draft.edges.push(DiagramEdge {
                from: format!("{branch}:false"),
                to: target,
                attrs: Vec::new(),
            });
            Ok(())
        }
        [_] => Err(diagram_error(
            line_no,
            column,
            "back edge needs a node before the branch target",
        )),
        _ => Err(diagram_error(
            line_no,
            column,
            "ambiguous diagram: back edge requires exactly one branch node",
        )),
    }
}

fn back_edge_marker(diagram_lines: &[SourceLine<'_>]) -> Option<(usize, usize)> {
    diagram_lines.iter().find_map(|(line_no, line)| {
        line.find('^')
            .or_else(|| line.find('+'))
            .map(|index| (*line_no, index + 1))
    })
}

fn apply_attrs(draft: &mut DiagramDraft, attrs_lines: &[SourceLine<'_>]) -> Result<()> {
    let node_index = draft
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), index))
        .collect::<BTreeMap<_, _>>();
    for (line_no, line) in attrs_lines {
        let trimmed = line.trim();
        let column = line.find(trimmed).map(|index| index + 1).unwrap_or(1);
        let Some((path, value)) = trimmed.split_once('=') else {
            return Err(diagram_error(*line_no, column, "expected attr = value"));
        };
        let path = path.trim();
        let value = value.trim();
        if path.is_empty() || value.is_empty() {
            return Err(diagram_error(*line_no, column, "empty attr path or value"));
        }
        apply_attr(draft, &node_index, *line_no, column, path, value)?;
    }
    Ok(())
}

fn apply_attr(
    draft: &mut DiagramDraft,
    node_index: &BTreeMap<String, usize>,
    line_no: usize,
    column: usize,
    path: &str,
    value: &str,
) -> Result<()> {
    let Some((subject, key)) = path.rsplit_once('.') else {
        return Err(diagram_error(
            line_no,
            column,
            "expected subject.field attr path",
        ));
    };
    if subject == "budget" {
        draft.budget_attrs.push((key.to_owned(), value.to_owned()));
        return Ok(());
    }
    if subject.contains(':') {
        let Some(edge) = draft.edges.iter_mut().find(|edge| edge.from == subject) else {
            return Err(diagram_error(
                line_no,
                column,
                format!("unknown edge attr subject {subject}"),
            ));
        };
        edge.attrs.push((key.to_owned(), value.to_owned()));
    } else {
        let Some(index) = node_index.get(subject).copied() else {
            return Err(diagram_error(
                line_no,
                column,
                format!("unknown node attr subject {subject}"),
            ));
        };
        draft.nodes[index]
            .attrs
            .push((key.to_owned(), value.to_owned()));
    }
    Ok(())
}

fn render_text(draft: &DiagramDraft) -> String {
    let mut out = String::from("topology diagram\n");
    for node in &draft.nodes {
        out.push_str("node ");
        out.push_str(&node.id);
        out.push_str(" verb=");
        out.push_str(&node.verb);
        push_attrs(&mut out, &node.attrs);
        out.push('\n');
    }
    for edge in &draft.edges {
        out.push_str("wire ");
        out.push_str(&edge.from);
        out.push_str(" -> ");
        out.push_str(&edge.to);
        push_attrs(&mut out, &edge.attrs);
        out.push('\n');
    }
    if !draft.budget_attrs.is_empty() {
        out.push_str("budget");
        push_attrs(&mut out, &draft.budget_attrs);
        out.push('\n');
    }
    out
}

fn push_attrs(out: &mut String, attrs: &[(String, String)]) {
    for (key, value) in attrs {
        out.push(' ');
        out.push_str(key);
        out.push('=');
        out.push_str(value);
    }
}

fn diagram_error(line: usize, column: usize, message: impl Into<String>) -> Error {
    Error::Eval(format!(
        "topology diagram parse error at line {line}, column {column}: {}",
        message.into()
    ))
}
