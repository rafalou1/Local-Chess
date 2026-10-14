use serde::{Deserialize, Serialize};
use shakmaty::{
    fen::Fen, san::SanPlus, uci::UciMove, CastlingMode, Chess, Color, EnPassantMode, Position, Role,
};
use std::time::Instant;
pub const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
#[derive(Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: usize,
    pub parent: Option<usize>,
    pub fen: String,
    pub uci: String,
    pub san: String,
    pub white_ms: f64,
    pub black_ms: f64,
    pub comment: String,
    #[serde(default)]
    pub nags: Vec<u8>,
    #[serde(default)]
    pub evidence: Option<serde_json::Value>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Game {
    #[serde(default)]
    pub ai_opponent: Option<crate::provider_play::Selection>,
    pub id: String,
    pub name: String,
    pub root: String,
    pub nodes: Vec<Node>,
    pub live: usize,
    pub revision: u64,
    pub human: String,
    pub difficulty: u32,
    pub base_ms: f64,
    pub increment_ms: f64,
    pub result: Option<String>,
    pub chess960: bool,
    pub assistance: u32,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub tags: std::collections::BTreeMap<String, String>,
    #[serde(skip)]
    pub tick: Option<Instant>,
}
#[derive(Serialize)]
pub struct Snapshot {
    pub game: Game,
    pub node: usize,
    pub fen: String,
    pub pieces: Vec<Piece>,
    pub legal: Vec<String>,
    pub turn: String,
    pub check: Option<String>,
    pub status: Option<String>,
    pub white_ms: f64,
    pub black_ms: f64,
    pub paused: bool,
}
#[derive(Serialize)]
pub struct Piece {
    pub square: String,
    pub color: String,
    pub role: String,
}
pub fn position(fen: &str, c960: bool) -> Result<Chess, String> {
    fen.parse::<Fen>()
        .map_err(|e| e.to_string())?
        .into_position(if c960 {
            CastlingMode::Chess960
        } else {
            CastlingMode::Standard
        })
        .map_err(|e| e.to_string())
}
pub fn fen(p: &Chess) -> String {
    Fen::from_position(p, EnPassantMode::Legal).to_string()
}
pub fn uci(p: &Chess, m: &shakmaty::Move) -> String {
    UciMove::from_move(*m, p.castles().mode()).to_string()
}
impl Game {
    pub fn new(
        root: String,
        human: String,
        difficulty: u32,
        base_ms: f64,
        increment_ms: f64,
        chess960: bool,
    ) -> Result<Self, String> {
        let p = position(&root, chess960)?;
        let root = fen(&p);
        Ok(Self {
            ai_opponent: None,
            id: uuid::Uuid::new_v4().to_string(),
            name: "Learning game".into(),
            root: root.clone(),
            nodes: vec![Node {
                id: 0,
                parent: None,
                fen: root,
                uci: String::new(),
                san: String::new(),
                white_ms: base_ms,
                black_ms: base_ms,
                comment: String::new(),
                nags: vec![],
                evidence: None,
            }],
            live: 0,
            revision: 0,
            human,
            difficulty,
            base_ms,
            increment_ms,
            result: None,
            chess960,
            assistance: 0,
            paused: false,
            tags: std::collections::BTreeMap::new(),
            tick: None,
        })
    }
    pub fn path(&self, node: usize) -> Vec<usize> {
        let mut out = vec![];
        let mut at = Some(node);
        while let Some(i) = at {
            out.push(i);
            at = self.nodes[i].parent;
        }
        out.reverse();
        out
    }
    pub fn engine_position(&self, node: usize) -> String {
        let moves = self
            .path(node)
            .into_iter()
            .skip(1)
            .map(|i| self.nodes[i].uci.clone())
            .collect::<Vec<_>>()
            .join(" ");
        format!("position fen {} moves {}", self.root, moves)
    }
    pub fn remaining(&self) -> (f64, f64) {
        let n = &self.nodes[self.live];
        let elapsed = self
            .tick
            .map(|i| i.elapsed().as_secs_f64() * 1000.)
            .unwrap_or(0.);
        let white = position(&n.fen, self.chess960)
            .map(|p| p.turn() == Color::White)
            .unwrap_or(true);
        (
            n.white_ms - if white { elapsed } else { 0. },
            n.black_ms - if !white { elapsed } else { 0. },
        )
    }
    pub fn pause(&mut self) {
        let (w, b) = self.remaining();
        self.nodes[self.live].white_ms = w.max(0.);
        self.nodes[self.live].black_ms = b.max(0.);
        self.tick = None;
        self.paused = true;
    }
    pub fn timeout(p: &Chess, flagged: Color) -> String {
        let winner = !flagged;
        let b = p.board();
        let mine = b.by_color(winner);
        let insufficient = mine.count() == 1
            || (mine.count() == 2 && (mine & (b.bishops() | b.knights())).count() == 1)
            || (mine.count() == 3
                && (mine & b.knights()).count() == 2
                && b.by_color(flagged).count() == 1);
        if insufficient {
            "Draw by timeout versus insufficient material (online policy)".into()
        } else {
            format!(
                "{} wins on time",
                if winner == Color::White {
                    "White"
                } else {
                    "Black"
                }
            )
        }
    }
    pub fn material_draw(p: &Chess) -> bool {
        if p.is_insufficient_material() {
            return true;
        }
        let b = p.board();
        let minor_only = |c: Color| {
            let mine = b.by_color(c);
            mine.count() <= 2 && (mine & (b.pawns() | b.rooks() | b.queens())).is_empty()
        };
        if minor_only(Color::White) && minor_only(Color::Black) {
            return true;
        }
        for side in [Color::White, Color::Black] {
            let mine = b.by_color(side);
            let other = b.by_color(!side);
            if other.count() == 1 && mine.count() == 3 && (b.knights() & mine).count() == 2 {
                return true;
            }
        }
        false
    }
    pub fn adjudicate(&self, p: &Chess) -> Option<String> {
        if p.is_checkmate() {
            return Some(format!(
                "{} wins by checkmate",
                if p.turn() == Color::White {
                    "Black"
                } else {
                    "White"
                }
            ));
        }
        if p.is_stalemate() {
            return Some("Draw by stalemate".into());
        }
        if Self::material_draw(p) {
            return Some("Draw by insufficient material (online policy)".into());
        }
        if p.halfmoves() >= 100 {
            return Some("Draw by 50-move rule".into());
        }
        let key = fen(p)
            .split_whitespace()
            .take(4)
            .collect::<Vec<_>>()
            .join(" ");
        let count = self
            .path(self.live)
            .iter()
            .filter(|&&i| {
                self.nodes[i]
                    .fen
                    .split_whitespace()
                    .take(4)
                    .collect::<Vec<_>>()
                    .join(" ")
                    == key
            })
            .count();
        if count >= 3 {
            return Some("Draw by threefold repetition".into());
        }
        None
    }
    pub fn snapshot(&mut self, node: Option<usize>) -> Result<Snapshot, String> {
        let node = node.unwrap_or(self.live);
        let n = self.nodes.get(node).ok_or("Unknown node")?;
        let p = position(&n.fen, self.chess960)?;
        let (w, b) = self.remaining();
        if self.base_ms > 0. && self.result.is_none() && (w <= 0. || b <= 0.) {
            let live_pos = position(&self.nodes[self.live].fen, self.chess960)?;
            self.result = Some(Self::timeout(
                &live_pos,
                if w <= 0. { Color::White } else { Color::Black },
            ));
            self.tick = None;
        }
        let pieces = p
            .board()
            .iter()
            .map(|(s, p)| Piece {
                square: s.to_string(),
                color: if p.color == Color::White { "w" } else { "b" }.into(),
                role: match p.role {
                    Role::Pawn => "p",
                    Role::Knight => "n",
                    Role::Bishop => "b",
                    Role::Rook => "r",
                    Role::Queen => "q",
                    Role::King => "k",
                }
                .into(),
            })
            .collect();
        Ok(Snapshot {
            game: self.clone(),
            node,
            fen: fen(&p),
            pieces,
            legal: p.legal_moves().iter().map(|m| uci(&p, m)).collect(),
            turn: if p.turn() == Color::White { "w" } else { "b" }.into(),
            check: if p.is_check() {
                p.board().king_of(p.turn()).map(|s| s.to_string())
            } else {
                None
            },
            status: self.result.clone().or_else(|| self.adjudicate(&p)),
            white_ms: w.max(0.),
            black_ms: b.max(0.),
            paused: self.paused,
        })
    }
    pub fn commit(&mut self, text: &str, revision: u64, premove: bool) -> Result<(), String> {
        if revision != self.revision {
            return Err("Position changed; stale move rejected".into());
        }
        if self.result.is_some() {
            return Err("Game ended".into());
        }
        if self.paused {
            return Err("Game is paused; resume before moving".into());
        }
        let mut p = position(&self.nodes[self.live].fen, self.chess960)?;
        let m = text
            .parse::<UciMove>()
            .map_err(|e| e.to_string())?
            .to_move(&p)
            .map_err(|e| e.to_string())?;
        let (mut w, mut b) = self.remaining();
        if self.base_ms > 0. {
            let time = if p.turn() == Color::White {
                &mut w
            } else {
                &mut b
            };
            if *time <= 100. || (premove && *time < 200.) {
                self.result = Some(Self::timeout(&p, p.turn()));
                self.tick = None;
                return Err("Insufficient time for move (100 ms floor policy)".into());
            }
            if premove {
                *time -= 100.;
            }
            *time += self.increment_ms;
        }
        let san = SanPlus::from_move(p.clone(), m).to_string();
        let uci = uci(&p, &m);
        p.play_unchecked(m);
        let id = self.nodes.len();
        self.nodes.push(Node {
            id,
            parent: Some(self.live),
            fen: fen(&p),
            uci,
            san,
            white_ms: w,
            black_ms: b,
            comment: String::new(),
            nags: vec![],
            evidence: None,
        });
        self.live = id;
        self.revision += 1;
        self.result = self.adjudicate(&p);
        self.tick = if self.base_ms > 0. && self.result.is_none() {
            Some(Instant::now())
        } else {
            None
        };
        Ok(())
    }
    pub fn resign(&mut self) {
        let white = if self.human == "both" {
            position(&self.nodes[self.live].fen, self.chess960)
                .map(|p| p.turn() == Color::White).unwrap_or(true)
        } else { self.human == "w" };
        self.pause();
        self.result = Some(format!("{} wins by resignation", if white { "Black" } else { "White" }));
        self.revision += 1;
    }
    pub fn undo(&mut self) {
        self.pause();
        if let Some(parent) = self.nodes[self.live].parent {
            self.live = parent;
            if self.human != "both" && position(&self.nodes[self.live].fen, self.chess960)
                .map(|p| if p.turn() == Color::White { "w" } else { "b" } != self.human)
                .unwrap_or(false)
            {
                if let Some(p) = self.nodes[self.live].parent {
                    self.live = p;
                }
            }
        }
        self.result = None;
        self.revision += 1;
        self.assistance += 1;
        self.paused = false;
        if self.base_ms > 0. {
            self.tick = Some(Instant::now())
        }
    }
    pub fn legal_line(&self, node: usize, line: &[String]) -> Result<Vec<String>, String> {
        let mut p = position(&self.nodes[node].fen, self.chess960)?;
        let mut out = vec![];
        for text in line.iter().take(12) {
            let m = text
                .parse::<UciMove>()
                .map_err(|e| e.to_string())?
                .to_move(&p)
                .map_err(|e| e.to_string())?;
            out.push(SanPlus::from_move(p.clone(), m).to_string());
            p.play_unchecked(m);
        }
        Ok(out)
    }
    pub fn validate_backup(&mut self) -> Result<(), String> {
        if self.nodes.is_empty() || self.nodes.len() > 100000 || self.live >= self.nodes.len() {
            return Err("Invalid game tree size or active node".into());
        }
        let root = position(&self.root, self.chess960)?;
        if self.nodes[0].parent.is_some() || self.nodes[0].fen != fen(&root) {
            return Err("Root position mismatch".into());
        }
        for i in 0..self.nodes.len() {
            let n = &self.nodes[i];
            if n.id != i || !n.white_ms.is_finite() || !n.black_ms.is_finite() {
                return Err(format!("Invalid node {i}"));
            }
            if i > 0 {
                let parent = n.parent.ok_or("Missing parent")?;
                if parent >= i {
                    return Err("Invalid parent ordering".into());
                }
                let mut p = position(&self.nodes[parent].fen, self.chess960)?;
                let m = n
                    .uci
                    .parse::<UciMove>()
                    .map_err(|e| e.to_string())?
                    .to_move(&p)
                    .map_err(|e| format!("Illegal move in node {i}: {e}"))?;
                let san = SanPlus::from_move(p.clone(), m).to_string();
                p.play_unchecked(m);
                if fen(&p) != n.fen {
                    return Err(format!("Position does not match move history at node {i}"));
                }
                self.nodes[i].san = san;
            } else if n.id != 0 {
                return Err("Root ID mismatch".into());
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn perft(p: Chess, d: u8) -> u64 {
        if d == 0 {
            return 1;
        }
        p.legal_moves()
            .iter()
            .map(|m| {
                let mut q = p.clone();
                q.play_unchecked(*m);
                perft(q, d - 1)
            })
            .sum()
    }
    #[test]
    fn initial_perft() {
        assert_eq!(perft(Chess::default(), 3), 8902)
    }
    #[test]
    fn history_repetition() {
        let mut g = Game::new(START.into(), "w".into(), 400, 0., 0., false).unwrap();
        for m in [
            "g1f3", "g8f6", "f3g1", "f6g8", "g1f3", "g8f6", "f3g1", "f6g8",
        ] {
            g.commit(m, g.revision, false).unwrap()
        }
        assert!(g.result.unwrap().contains("threefold"));
    }
    #[test]
    fn stale_illegal() {
        let mut g = Game::new(START.into(), "w".into(), 400, 0., 0., false).unwrap();
        assert!(g.commit("e2e5", 0, false).is_err());
        g.commit("e2e4", 0, false).unwrap();
        assert!(g.commit("e7e5", 0, false).is_err());
    }
    #[test]
    fn online_two_knights() {
        let p = position("7k/8/8/8/8/8/8/KNN5 w - - 0 1", false).unwrap();
        assert!(Game::material_draw(&p));
    }
    #[test]
    fn undo_keeps_branch() {
        let mut g = Game::new(START.into(), "w".into(), 400, 0., 0., false).unwrap();
        g.commit("e2e4", 0, false).unwrap();
        g.commit("e7e5", 1, false).unwrap();
        g.undo();
        assert_eq!(g.live, 0);
        g.commit("d2d4", g.revision, false).unwrap();
        assert_eq!(g.nodes.len(), 4);
    }
    #[test]
    fn pinned_ep() {
        let p = position("k7/8/8/4KPpr/8/8/8/8 w - g6 0 1", false).unwrap();
        assert!("f5g6".parse::<UciMove>().unwrap().to_move(&p).is_err())
    }
    #[test]
    fn both_takeback_is_one_ply_and_preserves_the_recorded_branch() {
        let mut g = Game::new(START.into(), "both".into(), 400, 0., 0., false).unwrap();
        g.commit("e2e4",0,false).unwrap();
        g.commit("e7e5",1,false).unwrap();
        g.undo();
        assert_eq!(g.live,1);
        assert_eq!(g.nodes.len(),3);
        g.undo();
        assert_eq!(g.live,0);
    }
    #[test]
    fn manual_resignation_uses_the_actual_side_to_move() {
        for human in ["w","b","both"] {
            for black_turn in [false,true] {
                let mut g=Game::new(START.into(),human.into(),400,0.,0.,false).unwrap();
                if black_turn {g.commit("e2e4",0,false).unwrap();}
                let count=g.nodes.len();
                g.resign();
                let white_resigns=human=="w" || (human=="both" && !black_turn);
                assert_eq!(g.result.as_deref(),Some(if white_resigns {"Black wins by resignation"}else{"White wins by resignation"}));
                assert_eq!(g.nodes.len(),count);
            }
        }
    }
}
