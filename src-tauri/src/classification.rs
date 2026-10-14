//! Conservative special labels. Every tactical candidate is replayed by shakmaty.
use crate::{
    engine::{Line, Search},
    game::{position, Game},
    run_engine, App,
};
use shakmaty::{uci::UciMove, Color, Move, Position, Role};
fn value(r: Role) -> i32 {
    match r {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 100,
    }
}
fn points(line: &Line, white: bool) -> Option<f64> {
    if line.bound {
        return None;
    }
    line.wdl.map(|w| {
        if white {
            (w[0] + w[1] / 2.) / 1000.
        } else {
            (w[2] + w[1] / 2.) / 1000.
        }
    })
}
// Human significance must use the same model as ordinary human move feedback.
// Perfect-play WDL remains exclusive to opponent opening safety below.
fn unique_human_move(best:&Search,played:&str,white:bool,min_depth:u32)->bool {
    let (Some(top),Some(second))=(best.lines.first(),best.lines.get(1)) else {return false;};
    if best.best!=played || top.pv.first().map(String::as_str)!=Some(played)
        || second.pv.first()==top.pv.first() || second.pv.is_empty()
        || top.depth<min_depth || second.depth<min_depth.saturating_sub(2) {return false;}
    match (crate::feedback_model::expected(top,white),crate::feedback_model::expected(second,white)) {
        // The least-bad move in a clearly inferior position is not a critical
        // "only good move". Near equality and better can qualify.
        (Some(first),Some(next))=>first>=0.45 && first-next>=0.15,
        _=>false,
    }
}
fn sacrifice(g: &Game, node: usize, played: &Search) -> bool {
    let Some(parent) = g.nodes[node].parent else {
        return false;
    };
    let Ok(before) = position(&g.nodes[parent].fen, g.chess960) else {
        return false;
    };
    let Ok(after) = position(&g.nodes[node].fen, g.chess960) else {
        return false;
    };
    let Some(line) = played.lines.first() else {
        return false;
    };
    let Ok(m) = g.nodes[node]
        .uci
        .parse::<UciMove>()
        .ok()
        .and_then(|u| u.to_move(&before).ok())
        .ok_or(())
    else {
        return false;
    };
    // A sound sacrifice can be accepted by the engine's main reply or offered
    // on a square where an opponent can legally take it. The best defense may
    // decline the material because accepting loses to a tactic, so requiring
    // the PV itself to capture the piece hid those verified sacrifices.
    let accepted = line.pv.get(1)
        .and_then(|reply| reply.parse::<UciMove>().ok())
        .and_then(|reply| reply.to_move(&after).ok())
        .filter(|reply| reply.to() == m.to() && reply.is_capture());
    let capture_move = accepted.clone().or_else(|| after.legal_moves().into_iter()
        .filter(|reply| reply.to() == m.to() && reply.is_capture())
        .max_by_key(|reply| match reply {
            Move::Normal { capture: Some(c), .. } => value(*c),
            Move::EnPassant { .. } => value(Role::Pawn),
            _ => 0,
        }));
    let Some(capture_move) = capture_move else {return false;};
    let given = match capture_move {
        Move::Normal { capture: Some(c), .. } => c,
        Move::EnPassant { .. } => Role::Pawn,
        _ => return false,
    };
    let gained = match m {
        Move::Normal {
            capture: Some(c), ..
        } => value(c),
        _ => 0,
    };
    if given == Role::Pawn || value(given) - gained < 2 { return false; }
    // A normal capture/recapture is a trade, not a sacrifice. Replay the
    // engine's next move and subtract material recovered immediately.
    if accepted.is_some() {
      if let Some(next) = line.pv.get(2) {
        let Ok(continuation)=after.clone().play(capture_move) else {return false;};
        let Some(recapture)=next.parse::<UciMove>().ok().and_then(|u|u.to_move(&continuation).ok()) else {return false;};
        let recovered=if recapture.to()==m.to() {match recapture {Move::Normal {capture:Some(c),..}=>value(c),Move::EnPassant {..}=>1,_=>0}} else {0};
        if value(given)-gained-recovered<2 {return false;}
      }
    }
    true
}
// A sound sacrifice can preserve equality and can be the only winning move.
// Chess.com's published rule does not require uniqueness. Judge "winning even
// without it" by the best distinct alternative, never by the sacrifice itself.
fn preliminary_sound_sacrifice(played:&Search,white:bool)->bool {
    played.lines.first().and_then(|l|crate::feedback_model::expected(l,white)).is_some_and(|score|score>=0.50)
}
fn brilliant_position(best:&Search,played:&Search,white:bool)->bool {
    let Some(line)=played.lines.first() else {return false;};
    let Some(score)=crate::feedback_model::expected(line,white) else {return false;};
    if score<0.50 {return false;}
    let Some(move_uci)=line.pv.first() else {return false;};
    let mut alternatives=Vec::new();
    for other in best.lines.iter().filter(|l|l.pv.first()!=Some(move_uci)) {
        // An unknown or bounded stronger alternative cannot be replaced by a
        // weaker known one to manufacture a special acknowledgement.
        if other.pv.is_empty() || other.depth<8 {return false;}
        let Some(score)=crate::feedback_model::expected(other,white) else {return false;};
        alternatives.push(score);
    }
    alternatives.into_iter().max_by(|a,b|a.total_cmp(b)).is_some_and(|score|score<0.95)
}
pub fn refine(
    a: &App,
    g: &Game,
    node: usize,
    best: &Search,
    played: &Search,
    loss: f64,
    refinement_ms: u32,
) -> Result<Option<&'static str>, String> {
    let Some(parent) = g.nodes[node].parent else {
        return Ok(None);
    };
    let before = position(&g.nodes[parent].fen, g.chess960)?;
    let white = before.turn() == Color::White;
    let (Some(b), Some(p)) = (best.lines.first(), played.lines.first()) else {
        return Ok(None);
    };
    let (Some(bp), Some(pp)) = (crate::feedback_model::expected(b, white), crate::feedback_model::expected(p, white)) else {
        return Ok(None);
    };
    // Miss needs an established opportunity after the opponent's analyzed error.
    if g.nodes[parent]
        .evidence
        .as_ref()
        .and_then(|v| v["loss"].as_f64())
        .unwrap_or(0.)
        >= 0.20
        && bp >= 0.80
        && pp < 0.60
    {
        return Ok(Some("Miss"));
    }
    if loss >= 0.02 || before.legal_moves().len() < 2 {
        return Ok(None);
    }
    // Preliminary alternatives may be bounded when the short search stops.
    // They trigger verification, never decide the final special label.
    let sac = sacrifice(g, node, played) && preliminary_sound_sacrifice(played,white);
    let unique = unique_human_move(best,&g.nodes[node].uci,white,8);
    if !sac && !unique {
        return Ok(None);
    }
    // Only rare special candidates pay for a deeper independent pair of searches.
    let deep_best = run_engine(a, g, parent, false, refinement_ms, 3, None)?;
    let deep_played = if let Some(candidate)=crate::same_root_candidate(g,node,&deep_best) {
        candidate
    } else {run_engine(a,g,parent,false,refinement_ms,1,Some(&g.nodes[node].uci))?};
    let (Some(db), Some(dp)) = (deep_best.lines.first(), deep_played.lines.first()) else {
        return Ok(None);
    };
    let (Some(dbp), Some(dpp)) = (crate::feedback_model::expected(db, white), crate::feedback_model::expected(dp, white)) else {
        return Ok(None);
    };
    if db.depth < 16 || dp.depth < 16 || dbp - dpp >= 0.02 || ((bp - pp) - (dbp - dpp)).abs() > 0.03
    {
        return Ok(None);
    }
    if sac && sacrifice(g, node, &deep_played) && brilliant_position(&deep_best,&deep_played,white) {
        return Ok(Some("Brilliant"));
    }
    if unique && unique_human_move(&deep_best,&g.nodes[node].uci,white,16)
    {
        return Ok(Some("Great"));
    }
    Ok(None)
}
/// Opponent opening choices retain an independent perfect-play safety filter.
pub fn opening_candidates(g: &Game, search: &Search) -> Vec<String> {
    let Ok(p) = position(&g.nodes[g.live].fen, g.chess960) else {
        return vec![];
    };
    let white = p.turn() == Color::White;
    let Some(best) = search.lines.first().and_then(|l| points(l, white)) else {
        return vec![];
    };
    search
        .lines
        .iter()
        .filter_map(|line| {
            if points(line, white)? < best - 0.05 {
                return None;
            }
            let text = line.pv.first()?;
            let mut next = g.clone();
            next.commit(text, g.revision, false).ok()?;
            book(&next, next.live).then(|| text.clone())
        })
        .collect()
}
/// Known legal edges from the pinned CC0 opening corpus; engine safety remains separate.
pub fn book(g: &Game, node: usize) -> bool {
    crate::openings::is_book(g,node)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn search_for(cp:i32,second:i32)->Search {
        Search {lines:vec![Line{depth:18,cp:Some(cp),pv:vec!["c1h6".into()],..Default::default()},Line{depth:18,cp:Some(second),pv:vec!["c1e3".into()],..Default::default()}],best:"c1h6".into(),elapsed_ms:100.,engine:"Stockfish 19".into()}
    }
    #[test]
    fn brilliant_accepts_equality_and_multiple_good_moves_but_not_already_winning() {
        let equal=search_for(0,0);
        assert!(brilliant_position(&equal,&equal,true));
        let only_win=search_for(950,0);
        assert!(brilliant_position(&only_win,&only_win,true),"winning only with sacrifice is eligible");
        let already_won=search_for(950,850);
        assert!(!brilliant_position(&already_won,&already_won,true));
        let losing=search_for(-150,-150);
        assert!(!brilliant_position(&losing,&losing,true));
        let mut bounded=equal.clone(); bounded.lines[0].bound=true;
        assert!(!brilliant_position(&equal,&bounded,true));
        let mut unknown=equal.clone(); unknown.lines[1].bound=true;
        assert!(preliminary_sound_sacrifice(&equal,true),"bounded preliminary alternatives must still trigger deeper proof");
        assert!(!brilliant_position(&unknown,&equal,true),"bounded stronger alternatives cannot be skipped in final proof");
        unknown=equal.clone(); unknown.lines.truncate(1);
        assert!(!brilliant_position(&unknown,&unknown,true),"unknown alternatives cannot establish eligibility");
    }
    #[test]
    fn ordinary_capture_recapture_is_not_a_sacrifice() {
        let mut g=Game::new("4k3/8/8/3r4/3R4/8/8/4K3 w - - 0 1".into(),"w".into(),400,0.,0.,false).unwrap();
        g.commit("d4d5",0,false).unwrap();
        let s=Search{lines:vec![Line {pv:vec!["d4d5".into(),"e8e7".into()],..Default::default()}],best:"d4d5".into(),elapsed_ms:0.,engine:"test".into()};
        assert!(!sacrifice(&g,1,&s));
    }
    #[test]
    fn sacrifice_replays_legal_acceptance_and_distinguishes_unrelated_tactical_gain() {
        for (fen,pv,expected) in [
            ("3rkb1r/p2nqppp/5n2/1B2p1B1/4P3/1Q6/P1P2P1P/2KR3R w k - 0 1",vec!["d1d7","d8d7","h1d1"],true),
            ("3rkb1r/p2nqppp/5n2/1B2p1B1/4P3/1Q6/PPP2P1P/2KR3R w k - 0 1",vec!["d1d7","f6d7","g5e7"],true),
            // The engine declines an offered rook because the capture loses
            // to the tactic; the legal, poisoned capture still proves a sac.
            ("3rkb1r/p2nqppp/5n2/1B2p1B1/4P3/1Q6/P1P2P1P/2KR3R w k - 0 1",vec!["d1d7","h8g8"],true),
            ("3k4/8/8/3r4/8/2B5/8/3RK3 w - - 0 1",vec!["d1d4","d5d4","c3d4"],false),
        ] {
            let mut g=Game::new(fen.into(),"w".into(),400,0.,0.,false).unwrap();
            let line:Vec<String>=pv.into_iter().map(str::to_string).collect();
            g.legal_line(0,&line).unwrap();
            g.commit(&line[0],0,false).unwrap();
            let s=Search {best:line[0].clone(),lines:vec![Line{pv:line,..Default::default()}],elapsed_ms:0.,engine:"test".into()};
            assert_eq!(sacrifice(&g,1,&s),expected);
        }
    }
    #[test]
    fn human_unique_move_does_not_disappear_when_perfect_wdl_saturates() {
        let top=Line {depth:18,cp:Some(-950),wdl:Some([0.,0.,1000.]),pv:vec!["g6h5".into()],..Default::default()};
        let second=Line {depth:18,cp:Some(-205),wdl:Some([0.,9.,991.]),pv:vec!["h6f5".into()],..Default::default()};
        let mut s=Search{lines:vec![top,second],best:"g6h5".into(),elapsed_ms:300.,engine:"Stockfish 19".into()};
        assert!(points(&s.lines[0],false).unwrap()-points(&s.lines[1],false).unwrap()<0.15);
        assert!(unique_human_move(&s,"g6h5",false,16));
        let mut losing=s.clone();losing.lines[0].cp=Some(250);losing.lines[1].cp=Some(950);
        assert!(!unique_human_move(&losing,"g6h5",false,16),"a uniquely least-bad losing choice is not Great");
        assert!(!unique_human_move(&s,"h6f5",false,16));
        s.lines[1].cp=Some(-850);
        assert!(!unique_human_move(&s,"g6h5",false,16),"two comfortably winning human choices are not unique");
        s.lines[1].cp=Some(-205);s.lines[1].bound=true;
        assert!(!unique_human_move(&s,"g6h5",false,16));
        s.lines[1].bound=false;s.lines[1].depth=13;
        assert!(!unique_human_move(&s,"g6h5",false,16));
        s.lines[1].depth=18;s.lines[1].pv=s.lines[0].pv.clone();
        assert!(!unique_human_move(&s,"g6h5",false,16));
    }
    #[test]
    fn opening_policy_requires_legal_index_and_safe_expected_score() {
        let mut g = Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
        g.commit("e2e4", 0, false).unwrap();
        let lines = vec![
            Line {
                pv: vec!["a7a6".into()],
                wdl: Some([0., 1000., 0.]),
                ..Default::default()
            },
            Line {
                pv: vec!["e7e5".into()],
                wdl: Some([0., 1000., 0.]),
                ..Default::default()
            },
            Line {
                pv: vec!["e7e6".into()],
                wdl: Some([1000., 0., 0.]),
                ..Default::default()
            },
            Line {
                pv: vec!["e7e4".into()],
                wdl: Some([0., 1000., 0.]),
                ..Default::default()
            },
        ];
        let search = Search {
            lines,
            best: "a7a6".into(),
            elapsed_ms: 400.,
            engine: "Stockfish19".into(),
        };
        assert_eq!(opening_candidates(&g, &search), vec!["a7a6", "e7e5"]);
    }
    #[test]
    fn book_recognizes_valid_st_george_opening() {
        let mut g = Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
        g.commit("e2e4", 0, false).unwrap();
        assert!(book(&g, 1));
        g.commit("a7a6", 1, false).unwrap();
        assert!(book(&g, 2));
        assert!(!book(&g, 0));
        assert!(!book(&g, usize::MAX));
    }
    #[test]
    fn opposite_sides_points() {
        let l = Line {
            wdl: Some([800., 100., 100.]),
            ..Default::default()
        };
        assert_eq!(points(&l, true), Some(0.85));
        assert_eq!(points(&l, false), Some(0.15));
    }
}
