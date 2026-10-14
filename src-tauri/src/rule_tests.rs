use crate::game::*;
use shakmaty::{uci::UciMove, Chess, Color, Position};
#[test]
fn forced_played_root_reuses_verified_child_without_mutation_or_score_changes() {
    let mut g=Game::new(START.into(),"both".into(),100,0.,0.,false).unwrap();
    g.commit("e2e4",0,false).unwrap();
    let original=serde_json::to_string(&g).unwrap();
    let mut search=crate::engine::Search{lines:vec![crate::engine::Line{
        depth:12,cp:Some(32),pv:vec!["e7e5".into(),"g1f3".into()],..Default::default()
    }],best:"e7e5".into(),elapsed_ms:60.,engine:"Stockfish 19".into()};
    let played=crate::played_from_child(&g,1,&search).unwrap();
    assert_eq!(played.lines[0].pv,vec!["e2e4","e7e5","g1f3"]);
    assert_eq!(played.lines[0].san,vec!["e4","e5","Nf3"]);
    assert_eq!(played.lines[0].cp,Some(32));
    assert_eq!(serde_json::to_string(&g).unwrap(),original);
    search.lines[0].pv=vec!["e7e4".into()];
    assert!(crate::played_from_child(&g,1,&search).is_none());
    search.lines[0].pv=vec!["e7e5".into()];search.lines[0].bound=true;
    assert!(crate::played_from_child(&g,1,&search).is_none());
    search.lines[0].bound=false;search.lines[0].depth=7;
    assert!(crate::played_from_child(&g,1,&search).is_none());
}
fn game(f: &str) -> Game {
    Game::new(f.into(), "both".into(), 400, 0., 0., false).unwrap()
}
fn perft(p: Chess, d: u8) -> u64 {
    if d == 0 {
        return 1;
    }
    p.legal_moves()
        .into_iter()
        .map(|m| {
            let mut q = p.clone();
            q.play_unchecked(m);
            perft(q, d - 1)
        })
        .sum()
}
#[test]
fn kiwipete_castling_perft() {
    let p = position(
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        false,
    )
    .unwrap();
    assert_eq!(perft(p, 3), 97862)
}
#[test]
fn both_castles() {
    for m in ["e1g1", "e1c1"] {
        let mut g = game("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        g.commit(m, 0, false).unwrap();
        assert!(g.nodes[1].san.starts_with("O-"));
    }
}
#[test]
fn castle_through_attack() {
    let p = position("4kr2/8/8/8/8/8/8/R3K2R w KQ - 0 1", false).unwrap();
    assert!("e1g1".parse::<UciMove>().unwrap().to_move(&p).is_err());
    assert!("e1c1".parse::<UciMove>().unwrap().to_move(&p).is_ok())
}
#[test]
fn rook_capture_removes_right() {
    let mut g = game("4k2r/6B1/8/8/8/8/8/4K3 w k - 0 1");
    g.commit("g7h8", 0, false).unwrap();
    assert_eq!(g.nodes[1].fen.split_whitespace().nth(2), Some("-"))
}
#[test]
fn all_promotion_choices() {
    for c in ["q", "r", "b", "n"] {
        let mut g = game("7k/P7/8/8/8/8/8/7K w - - 0 1");
        g.commit(&format!("a7a8{c}"), 0, false).unwrap();
        assert!(g.nodes[1].san.contains('='));
    }
}
#[test]
fn black_promotion() {
    let mut g = game("7k/8/8/8/8/8/p7/7K b - - 0 1");
    g.commit("a2a1n", 0, false).unwrap();
    assert!(g.nodes[1].san.contains("=N"));
}
#[test]
fn en_passant_capture() {
    let mut g = game("7k/8/8/3pP3/8/8/8/7K w - d6 0 1");
    g.commit("e5d6", 0, false).unwrap();
    assert!(g.nodes[1].fen.starts_with("7k/8/3P4/8"));
}
#[test]
fn mate_and_stalemate() {
    let mut mate = game("7k/8/5KQ1/8/8/8/8/8 w - - 0 1");
    mate.commit("g6g7", 0, false).unwrap();
    assert!(mate.result.unwrap().contains("checkmate"));
    let g = game("7k/5K2/6Q1/8/8/8/8/8 b - - 0 1");
    assert!(g
        .adjudicate(&position(&g.root, false).unwrap())
        .unwrap()
        .contains("stalemate"));
}
#[test]
fn halfmove_resets() {
    let mut g = game(START);
    g.commit("g1f3", 0, false).unwrap();
    assert_eq!(g.nodes[1].fen.split_whitespace().nth(4), Some("1"));
    g.commit("d7d5", 1, false).unwrap();
    assert_eq!(g.nodes[2].fen.split_whitespace().nth(4), Some("0"));
}
#[test]
fn fifty_move_rule() {
    let mut g = game("r6k/8/8/8/8/8/8/R6K w - - 99 50");
    g.commit("a1b1", 0, false).unwrap();
    assert!(g.result.unwrap().contains("50-move"));
}
#[test]
fn minor_policy_and_timeout() {
    let p = position("7k/7n/8/8/8/8/2B5/K7 w - - 0 1", false).unwrap();
    assert!(Game::material_draw(&p));
    assert!(Game::timeout(&p, Color::White).contains("Draw"));
    let p = position("7k/7r/8/8/8/8/8/K7 w - - 0 1", false).unwrap();
    assert!(Game::timeout(&p, Color::White).starts_with("Black wins"));
}
#[test]
fn low_time_premove_rejected_without_history() {
    let mut g = Game::new(START.into(), "w".into(), 400, 150., 0., false).unwrap();
    assert!(g.commit("e2e4", 0, true).is_err());
    assert_eq!(g.nodes.len(), 1);
    assert!(g.result.is_some());
}
#[test]
fn premove_charge_is_clock_time() {
    let mut g = Game::new(START.into(), "w".into(), 400, 1000., 0., false).unwrap();
    g.commit("e2e4", 0, true).unwrap();
    assert_eq!(g.nodes[1].white_ms, 900.);
}
#[test]
fn chess960_swap_castle() {
    let mut g = Game::new(
        "4k3/8/8/8/8/8/8/5KR1 w G - 0 1".into(),
        "w".into(),
        400,
        0.,
        0.,
        true,
    )
    .unwrap();
    g.commit("f1g1", 0, false).unwrap();
    assert!(g.nodes[1].fen.ends_with("b - - 1 1"));
    assert!(g.nodes[1].fen.starts_with("4k3/8/8/8/8/8/8/5RK1"));
}
