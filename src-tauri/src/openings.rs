//! CC0 opening positions and legal book edges, pinned and replay-validated at build time.
use crate::game::{Game, START};
use serde::Deserialize;
use std::{collections::{HashMap, HashSet}, sync::OnceLock};

#[derive(Deserialize)]
struct Index { positions: HashMap<String, Entry>, edges: HashSet<String> }
#[derive(Deserialize)]
struct Entry { name: String }
fn index() -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(|| serde_json::from_str(include_str!("../resources/openings/index.json")).expect("validated bundled opening index"))
}
fn key(fen: &str) -> String { fen.split_whitespace().take(4).collect::<Vec<_>>().join(" ") }
pub fn is_book(g: &Game, node: usize) -> bool {
    if g.root != START || g.chess960 { return false; }
    let Some(n) = g.nodes.get(node) else { return false; };
    let Some(parent) = n.parent.and_then(|p|g.nodes.get(p)) else { return false; };
    index().edges.contains(&format!("{}|{}",key(&parent.fen),n.uci))
}
pub fn name(g: &Game, mut node: usize) -> Option<&'static str> {
    if g.root != START || g.chess960 { return None; }
    loop {
        let n=g.nodes.get(node)?;
        if let Some(entry)=index().positions.get(&key(&n.fen)) { return Some(&entry.name); }
        node=n.parent?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn play(line: &str) -> Game {
        let mut g=Game::new(START.into(),"both".into(),400,0.,0.,false).unwrap();
        for m in line.split_whitespace(){g.commit(m,g.revision,false).unwrap();}
        g
    }
    #[test]
    fn scandinavian_book_and_latest_name_survive_offbook_move() {
        let mut g=play("e2e4 d7d5 e4d5 d8d5 b1c3 d5d8");
        for n in 1..=6 {assert!(is_book(&g,n),"ply {n}");}
        assert!(name(&g,6).unwrap().starts_with("Scandinavian Defense"));
        g.commit("h2h4",g.revision,false).unwrap();
        assert!(!is_book(&g,7));
        assert!(name(&g,7).unwrap().starts_with("Scandinavian Defense"));
    }
    #[test]
    fn opening_lookup_handles_transposition_and_guards_custom_roots() {
        let g=play("g1f3 d7d5 d2d4 g8f6 c2c4 e7e6");
        assert!(is_book(&g,g.live));assert!(name(&g,g.live).is_some());
        let mut custom=g.clone();custom.root=custom.nodes[1].fen.clone();
        assert!(!is_book(&custom,custom.live));assert!(name(&custom,custom.live).is_none());
        custom=g.clone();custom.chess960=true;assert!(!is_book(&custom,custom.live));
        assert!(!is_book(&g,usize::MAX));
    }
}
