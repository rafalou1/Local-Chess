use crate::{
    engine::Search,
    game::{position, Game},
};
use shakmaty::{uci::UciMove, Color, Move, Position, Role};
fn name(r: Role) -> &'static str {
    match r {
        Role::Pawn => "pawn",
        Role::Knight => "knight",
        Role::Bishop => "bishop",
        Role::Rook => "rook",
        Role::Queen => "queen",
        Role::King => "king",
    }
}
/// Short, conversational wording assembled only from the legal move and searched reply.
/// The evidence/PV remain available separately; no invented motive is attributed to a player.
pub fn friendly(
    g: &Game,
    node: usize,
    label: Option<&str>,
    alternative: Option<&str>,
    search: &Search,
) -> Result<String, String> {
    let n = &g.nodes[node];
    let Some(parent) = n.parent else {
        return Ok("Let's work on your chess as we play. Make your first move!".into());
    };
    let before = position(&g.nodes[parent].fen, g.chess960)?;
    let after = position(&n.fen, g.chess960)?;
    let m = n
        .uci
        .parse::<UciMove>()
        .map_err(|e| e.to_string())?
        .to_move(&before)
        .map_err(|e| e.to_string())?;
    let yours = g.human == "both" || (before.turn() == Color::White) == (g.human == "w");
    let bad = matches!(label, Some("Inaccuracy" | "Mistake" | "Blunder" | "Miss"));
    if bad && !yours {
        let mut text = "I've given you an opportunity. Let's see how you can use it.".to_owned();
        if let Some(line) = search.lines.first() {
            if let Some(text_move) = line.pv.first() {
                text_move
                    .parse::<UciMove>()
                    .map_err(|e| e.to_string())?
                    .to_move(&after)
                    .map_err(|e| e.to_string())?;
                if let Some(san) = line.san.first() {
                    text.push_str(&format!(" Look at {san}."));
                }
            }
        }
        return Ok(text);
    }
    let praise = if !yours {
        ""
    } else {
        match label {
            Some("Best") => ["That's the best move! ", "Well found! "][node % 2],
            Some("Excellent") => "Excellent! ",
            Some("Good") => "Good move. ",
            Some("Great") => "Great find! ",
            Some("Brilliant") => "Brilliant! ",
            Some("Book") if node <= 4 => "A good start! ",
            _ => "",
        }
    };
    if after.is_checkmate() {
        return Ok(if yours {
            "Checkmate! Well played — their king has no escape."
        } else {
            "Checkmate. Let's review the game and find where things changed."
        }
        .into());
    }
    let mut detail = match m {
        Move::Normal {
            role,
            to,
            capture: Some(c),
            ..
        } => format!(
            "{} {} takes the {} on {to}.",
            if yours { "Your" } else { "My" },
            name(role),
            name(c)
        ),
        Move::Normal {
            to,
            promotion: Some(p),
            ..
        } => format!(
            "{} pawn becomes a {} on {to}.",
            if yours { "Your" } else { "My" },
            name(p)
        ),
        Move::Castle { .. } => if yours {
            "Your king is now castled."
        } else {
            "I've castled. Your turn!"
        }
        .into(),
        Move::EnPassant { to, .. } => {
            format!("An en passant capture on {to} — the captured pawn comes off the board.")
        }
        Move::Normal {
            role: Role::Pawn,
            to,
            ..
        } if ["d4", "e4", "d5", "e5"].contains(&to.to_string().as_str()) => format!(
            "{} pawn takes a place in the centre on {to}.",
            if yours { "Your" } else { "My" }
        ),
        Move::Normal {
            role: Role::Knight | Role::Bishop,
            from,
            to,
            ..
        } if ["1", "8"].contains(&&from.to_string()[1..]) => format!(
            "{} brought {} {} into play on {to}.",
            if yours { "You've" } else { "I've" },
            if yours { "your" } else { "my" },
            name(m.role())
        ),
        _ => {
            if after.is_check() {
                "That's check. The king's attack has to be answered.".into()
            } else {
                format!("{} is on the board. Let's look at the reply.", n.san)
            }
        }
    };
    if bad {
        detail = match label {
            Some("Inaccuracy") => "There's a more precise move here.".into(),
            Some("Mistake") => "Let's take another look at that move.".into(),
            _ => "Careful — that gives your opponent a big opportunity.".into(),
        };
        if let Some(reply) = search.lines.first() {
            if let Some(uci) = reply.pv.first() {
                let r = uci
                    .parse::<UciMove>()
                    .map_err(|e| e.to_string())?
                    .to_move(&after)
                    .map_err(|e| e.to_string())?;
                if let Move::Normal {
                    to,
                    capture: Some(c),
                    ..
                } = r
                {
                    detail.push_str(&format!(
                        " {} can capture your {} on {to}.",
                        reply.san.first().unwrap_or(uci),
                        name(c)
                    ));
                }
            }
        }
        if let Some(alt) = alternative {
            detail.push_str(&format!(" Try {alt} instead."));
        }
    }
    Ok(format!("{praise}{detail}"))
}
pub fn consequence(g: &Game, node: usize) -> Result<Vec<String>, String> {
    let n = &g.nodes[node];
    let Some(parent) = n.parent else {
        return Ok(vec![]);
    };
    let before = position(&g.nodes[parent].fen, g.chess960)?;
    let after = position(&n.fen, g.chess960)?;
    let m = n
        .uci
        .parse::<UciMove>()
        .map_err(|e| e.to_string())?
        .to_move(&before)
        .map_err(|e| e.to_string())?;
    let side = if before.turn() == Color::White {
        "White"
    } else {
        "Black"
    };
    let mut facts = vec![];
    match m {
        Move::Normal {
            role,
            from,
            to,
            capture,
            promotion,
        } => {
            if let Some(c) = capture {
                facts.push(format!(
                    "{side}'s {} from {from} captured the {} on {to}.",
                    name(role),
                    name(c)
                ))
            }
            if let Some(p) = promotion {
                facts.push(format!("The pawn promoted to a {} on {to}.", name(p)))
            }
            if after.is_checkmate() {
                facts.push(format!(
                    "{} delivers checkmate: the opponent has no legal check evasion.",
                    n.san
                ))
            } else if after.is_check() {
                facts.push(format!(
                    "{} gives check, so the reply must address the king's attack.",
                    n.san
                ))
            } else if capture.is_none()
                && role == Role::Pawn
                && ["d4", "e4", "d5", "e5"].contains(&to.to_string().as_str())
            {
                facts.push(format!("The pawn now occupies {to}, a central square."))
            } else if [Role::Knight, Role::Bishop].contains(&role)
                && ["1", "8"].contains(&from.to_string()[1..].as_ref())
            {
                facts.push(format!(
                    "The {} moved from its back rank to {to}.",
                    name(role)
                ))
            }
        }
        Move::EnPassant { from, to } => facts.push(format!(
            "The pawn from {from} captured en passant on {to}; the passed pawn was removed."
        )),
        Move::Castle { king, rook } => facts.push(format!(
            "{side} castled with the king from {king} and rook from {rook}."
        )),
        _ => {}
    }
    Ok(facts)
}
pub fn line_facts(g: &Game, node: usize, search: &Search) -> Result<Vec<String>, String> {
    let Some(line) = search.lines.first() else {
        return Ok(vec![]);
    };
    let mut p = position(&g.nodes[node].fen, g.chess960)?;
    let mut facts = vec![];
    for (i, text) in line.pv.iter().take(4).enumerate() {
        let m = text
            .parse::<UciMove>()
            .map_err(|e| e.to_string())?
            .to_move(&p)
            .map_err(|e| e.to_string())?;
        let side = if p.turn() == Color::White {
            "White"
        } else {
            "Black"
        };
        let san = line.san.get(i).cloned().unwrap_or(text.clone());
        if let Move::Normal {
            role,
            to,
            capture: Some(c),
            ..
        } = m
        {
            facts.push(format!(
                "In the verified line, {san} lets {side}'s {} capture a {} on {to}.",
                name(role),
                name(c)
            ));
        }
        p.play_unchecked(m);
        if p.is_checkmate() {
            facts.push(format!(
                "The displayed continuation ends in checkmate after {san}."
            ));
            break;
        }
    }
    Ok(facts)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_fact_uses_actual_board() {
        let mut g = Game::new(
            "7k/8/8/8/3q4/2N5/8/1K6 w - - 0 1".into(),
            "w".into(),
            400,
            0.,
            0.,
            false,
        )
        .unwrap();
        g.commit("c3d5", 0, false).unwrap();
        let f = consequence(&g, 1).unwrap().join(" ");
        assert!(!f.contains("captured"));
    }
    #[test]
    fn en_passant_fact() {
        let mut g = Game::new(
            "7k/8/8/3pP3/8/8/8/K7 w - d6 0 1".into(),
            "w".into(),
            400,
            0.,
            0.,
            false,
        )
        .unwrap();
        g.commit("e5d6", 0, false).unwrap();
        assert!(consequence(&g, 1).unwrap()[0].contains("en passant"));
    }
}
