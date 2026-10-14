use crate::game::{position, uci, Game, START};
use pgn_reader::{Nag, Outcome, RawComment, RawTag, Reader, SanPlus, Skip, Visitor};
use std::{collections::BTreeMap, ops::ControlFlow};
struct Importer;
pub struct Text {
    g: Game,
    stack: Vec<usize>,
    main: usize,
}
impl Visitor for Importer {
    type Tags = BTreeMap<String, String>;
    type Movetext = Text;
    type Output = Result<Game, String>;
    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        ControlFlow::Continue(BTreeMap::new())
    }
    fn tag(&mut self, t: &mut Self::Tags, n: &[u8], v: RawTag<'_>) -> ControlFlow<Self::Output> {
        t.insert(
            String::from_utf8_lossy(n).into(),
            String::from_utf8_lossy(v.as_bytes()).into(),
        );
        ControlFlow::Continue(())
    }
    fn begin_movetext(&mut self, t: Self::Tags) -> ControlFlow<Self::Output, Text> {
        let c960 = t.get("Variant").map(|s| s == "Chess960").unwrap_or(false);
        let mut g = match Game::new(
            t.get("FEN").cloned().unwrap_or(START.into()),
            "both".into(),
            3200,
            0.,
            0.,
            c960,
        ) {
            Ok(g) => g,
            Err(e) => return ControlFlow::Break(Err(e)),
        };
        g.name = format!(
            "{} — {}",
            t.get("White").cloned().unwrap_or("White".into()),
            t.get("Black").cloned().unwrap_or("Black".into())
        );
        g.tags = t;
        ControlFlow::Continue(Text {
            g,
            stack: vec![],
            main: 0,
        })
    }
    fn san(&mut self, t: &mut Text, s: SanPlus) -> ControlFlow<Self::Output> {
        let p = match position(&t.g.nodes[t.g.live].fen, t.g.chess960) {
            Ok(p) => p,
            Err(e) => return ControlFlow::Break(Err(e)),
        };
        let m = match s.san.to_move(&p) {
            Ok(m) => m,
            Err(e) => return ControlFlow::Break(Err(format!("Illegal PGN move {s}: {e}"))),
        };
        t.g.result = None;
        if let Err(e) = t.g.commit(&uci(&p, &m), t.g.revision, false) {
            return ControlFlow::Break(Err(e));
        }
        if t.stack.is_empty() {
            t.main = t.g.live;
        }
        ControlFlow::Continue(())
    }
    fn nag(&mut self, t: &mut Text, n: Nag) -> ControlFlow<Self::Output> {
        t.g.nodes[t.g.live].nags.push(n.0);
        ControlFlow::Continue(())
    }
    fn comment(&mut self, t: &mut Text, c: RawComment<'_>) -> ControlFlow<Self::Output> {
        if !t.g.nodes[t.g.live].comment.is_empty() {
            t.g.nodes[t.g.live].comment.push(' ')
        }
        t.g.nodes[t.g.live]
            .comment
            .push_str(&String::from_utf8_lossy(c.as_bytes()));
        ControlFlow::Continue(())
    }
    fn begin_variation(&mut self, t: &mut Text) -> ControlFlow<Self::Output, Skip> {
        if t.stack.len() >= 64 {
            return ControlFlow::Break(Err("Variation nesting exceeds 64".into()));
        }
        t.stack.push(t.g.live);
        t.g.live = t.g.nodes[t.g.live].parent.unwrap_or(0);
        ControlFlow::Continue(Skip(false))
    }
    fn end_variation(&mut self, t: &mut Text) -> ControlFlow<Self::Output> {
        if let Some(n) = t.stack.pop() {
            t.g.live = n;
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break(Err("Unmatched variation closing parenthesis".into()))
        }
    }
    fn outcome(&mut self, t: &mut Text, o: Outcome) -> ControlFlow<Self::Output> {
        if t.stack.is_empty() {
            t.g.tags.insert("Result".into(), o.to_string());
        }
        ControlFlow::Continue(())
    }
    fn end_game(&mut self, mut t: Text) -> Self::Output {
        if !t.stack.is_empty() {
            return Err("Unclosed PGN variation".into());
        }
        t.g.live = t.main;
        t.g.result = match t.g.tags.get("Result").map(String::as_str) {
            Some("1-0") => Some("White wins (imported)".into()),
            Some("0-1") => Some("Black wins (imported)".into()),
            Some("1/2-1/2") => Some("Draw (imported)".into()),
            _ => None,
        };
        Ok(t.g)
    }
}
pub fn import(data: &str) -> Result<Vec<Game>, String> {
    if data.len() > 20_000_000 {
        return Err("PGN exceeds 20 MB".into());
    }
    let mut reader = Reader::new(data.as_bytes());
    let mut visitor = Importer;
    let mut games = vec![];
    while let Some(g) = reader.read_game(&mut visitor).map_err(|e| e.to_string())? {
        games.push(g?);
        if games.len() > 1000 {
            return Err("Import up to 1000 games at a time".into());
        }
    }
    if games.is_empty() {
        return Err("No PGN games found".into());
    }
    Ok(games)
}
pub fn export(g: &Game) -> String {
    let mut tags = g.tags.clone();
    tags.insert("Event".into(), g.name.clone());
    tags.insert("SetUp".into(), "1".into());
    tags.insert("FEN".into(), g.root.clone());
    if g.chess960 {
        tags.insert("Variant".into(), "Chess960".into());
    }
    let result = g
        .result
        .as_ref()
        .map(|r| {
            if r.starts_with("White") {
                "1-0"
            } else if r.starts_with("Black") {
                "0-1"
            } else {
                "1/2-1/2"
            }
        })
        .unwrap_or("*");
    tags.insert("Result".into(), result.into());
    let mut out = tags
        .iter()
        .map(|(k, v)| {
            format!(
                "[{k} \"{}\"]",
                v.replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace(['\n', '\r'], " ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    out.push_str("\n\n");
    let preferred = g.path(g.live);
    if !g.nodes[0].comment.is_empty() {
        out.push_str(&format!(
            "{{{}}} ",
            g.nodes[0].comment.replace(['{', '}'], "")
        ));
    }
    fn token(g: &Game, id: usize) -> String {
        let n = &g.nodes[id];
        let p = &g.nodes[n.parent.unwrap_or(0)];
        let fields = p.fen.split_whitespace().collect::<Vec<_>>();
        format!(
            "{}{} {} {}{}",
            fields[5],
            if fields[1] == "w" { "." } else { "..." },
            n.san,
            n.nags.iter().map(|n| format!("${n} ")).collect::<String>(),
            if n.comment.is_empty() {
                String::new()
            } else {
                format!("{{{}}} ", n.comment.replace(['{', '}'], ""))
            }
        )
    }
    fn children(g: &Game, parent: usize, preferred: &[usize], out: &mut String, depth: usize) {
        if depth > 10000 {
            return;
        }
        let ids = g
            .nodes
            .iter()
            .filter(|n| n.parent == Some(parent))
            .map(|n| n.id)
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return;
        }
        let first = ids
            .iter()
            .copied()
            .find(|n| preferred.contains(n))
            .unwrap_or(ids[0]);
        out.push_str(&token(g, first));
        out.push(' ');
        for id in ids.iter().filter(|&&n| n != first) {
            out.push('(');
            out.push_str(&token(g, *id));
            children(g, *id, preferred, out, depth + 1);
            out.push_str(") ");
        }
        children(g, first, preferred, out, depth + 1)
    }
    children(g, 0, &preferred, &mut out, 0);
    out.push_str(result);
    out.push('\n');
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_variations_unicode() {
        let data="[White \"Räphael\"]\n[Black \"Rowan\"]\n\n1. e4 {center} (1. d4 d5 (1... Nf6)) e5 2. Nf3 $1 *";
        let g = import(data).unwrap().remove(0);
        assert_eq!(g.nodes.len(), 7);
        let text = export(&g);
        let h = import(&text).unwrap().remove(0);
        assert_eq!(g.nodes.len(), h.nodes.len());
        assert!(text.contains("center"));
        assert!(text.contains("Räphael"));
        assert!(text.contains("$1"));
    }
    #[test]
    fn invalid_move() {
        assert!(import("1. e5 *").is_err())
    }
    #[test]
    fn multiple_games() {
        assert_eq!(import("1. e4 *\n\n1. d4 *").unwrap().len(), 2)
    }
}
