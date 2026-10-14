use crate::{
    coaching,
    engine::Search,
    game::{position, Game},
    App,
};
use shakmaty::{uci::UciMove, Move, Position, Role};
fn de_object(r: Role, opposing: bool) -> String {
    let article = if r == Role::Queen {
        if opposing {
            "die gegnerische"
        } else {
            "die"
        }
    } else if opposing {
        "den gegnerischen"
    } else {
        "den"
    };
    let name = if r == Role::Pawn {
        "Bauern"
    } else {
        role(r, true)
    };
    format!("{article} {name}")
}
pub fn language(a: &App) -> String {
    let saved: String =
        a.db.lock()
            .unwrap()
            .query_row(
                "SELECT value FROM settings WHERE key='preferences'",
                [],
                |r| r.get(0),
            )
            .unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&saved)
        .ok()
        .and_then(|v| v["language"].as_str().map(str::to_owned))
        .unwrap_or("en".into())
}
fn role(r: Role, de: bool) -> &'static str {
    match (r, de) {
        (Role::Pawn, false) => "pawn",
        (Role::Knight, false) => "knight",
        (Role::Bishop, false) => "bishop",
        (Role::Rook, false) => "rook",
        (Role::Queen, false) => "queen",
        (Role::King, false) => "king",
        (Role::Pawn, true) => "Bauer",
        (Role::Knight, true) => "Springer",
        (Role::Bishop, true) => "Läufer",
        (Role::Rook, true) => "Turm",
        (Role::Queen, true) => "Dame",
        (Role::King, true) => "König",
    }
}
pub fn technical_friendly(
    g: &Game,
    node: usize,
    label: Option<&str>,
    alt: Option<&str>,
    search: &Search,
    lang: &str,
) -> Result<String, String> {
    if matches!(label, Some("Inaccuracy" | "Mistake" | "Blunder" | "Miss")) {
        return mistake_reason(g, node, alt, search, lang);
    }
    if lang != "de" {
        let mut text = coaching::friendly(g, node, label, alt, search)?;
        if !matches!(label, Some("Inaccuracy" | "Mistake" | "Blunder" | "Miss")) {
            if let Some((_, _, ray)) = verified_ray(g, node, false)? {
                text = ray;
            } else if let Some(development) = development_line(g, node, search, false)? {
                text.push_str(&format!(" {development}"));
            }
        }
        return Ok(text);
    }
    let n = &g.nodes[node];
    let Some(parent) = n.parent else {
        return Ok("Wir verbessern dein Schach beim Spielen. Bereit für deinen ersten Zug?".into());
    };
    let before = position(&g.nodes[parent].fen, g.chess960)?;
    let after = position(&n.fen, g.chess960)?;
    let m = n
        .uci
        .parse::<UciMove>()
        .map_err(|e| e.to_string())?
        .to_move(&before)
        .map_err(|e| e.to_string())?;
    let yours = (before.turn() == shakmaty::Color::White) == (g.human != "b");
    if after.is_checkmate() {
        return Ok(if yours {
            "Schachmatt! Gut gespielt — der König hat kein Entkommen."
        } else {
            "Schachmatt. Lass uns gemeinsam anschauen, wo sich die Partie entschieden hat."
        }
        .into());
    }
    if matches!(label, Some("Inaccuracy" | "Mistake" | "Blunder" | "Miss")) {
        if !yours {
            let reply = search.lines.first().and_then(|l| l.san.first());
            return Ok(match reply {
                Some(san) => format!(
                    "Ich habe dir eine Chance gegeben. Schau dir {san} an.{}",
                    reply_consequence(g, node, search, true)?
                        .map(|s| format!(" {s}"))
                        .unwrap_or_default()
                ),
                None => "Ich habe dir eine Chance gegeben. Prüfen wir deine Möglichkeiten.".into(),
            });
        }
        let mut text = if label == Some("Inaccuracy") {
            "Hier gibt es einen genaueren Zug."
        } else {
            "Vorsicht — dieser Zug gibt deinem Gegner eine Chance."
        }
        .to_owned();
        if let Some(alt) = alt {
            text.push_str(&format!(" Versuche stattdessen {alt}."));
        }
        if let Some(detail) = reply_consequence(g, node, search, true)? {
            text.push_str(&format!(" {detail}"));
        }
        return Ok(text);
    }
    let prefix = if !yours {
        ""
    } else {
        match label {
            Some("Best") => "Das ist der beste Zug! ",
            Some("Excellent") => "Ausgezeichnet! ",
            Some("Good") => "Guter Zug. ",
            Some("Great") => "Stark gefunden! ",
            Some("Brilliant") => "Brillant! ",
            _ => "",
        }
    };
    let mut detail = match m {
        Move::Normal {
            role: r,
            to,
            capture: Some(c),
            ..
        } => format!(
            "{} {} schlägt {} auf {to}.",
            if yours {
                if r == Role::Queen {
                    "Deine"
                } else {
                    "Dein"
                }
            } else if r == Role::Queen {
                "Meine"
            } else {
                "Mein"
            },
            role(r, true),
            de_object(c, true)
        ),
        Move::Normal {
            to,
            promotion: Some(p),
            ..
        } => format!(
            "Der Bauer wird auf {to} zu einer neuen Figur: {}.",
            role(p, true)
        ),
        Move::Castle { .. } => "Der König hat rochiert.".into(),
        Move::EnPassant { to, .. } => {
            format!("En passant auf {to} — der benachbarte gegnerische Bauer wird geschlagen.")
        }
        Move::Normal {
            role: Role::Pawn,
            to,
            ..
        } if ["d4", "e4", "d5", "e5"].contains(&to.to_string().as_str()) => format!(
            "{} Bauer steht jetzt im Zentrum auf {to}.",
            if yours { "Dein" } else { "Mein" }
        ),
        Move::Normal {
            role: r, from, to, ..
        } if [Role::Knight, Role::Bishop].contains(&r)
            && ["1", "8"].contains(&&from.to_string()[1..]) =>
        {
            format!(
                "{} {} kommt auf {to} ins Spiel.",
                if yours { "Dein" } else { "Mein" },
                role(r, true)
            )
        }
        _ => {
            if after.is_check() {
                "Das ist Schach. Der Angriff auf den König muss beantwortet werden.".into()
            } else {
                format!("{0} ist gespielt. Schauen wir auf die Antwort.", n.san)
            }
        }
    };
    if let Some((_, _, text)) = verified_ray(g, node, true)? {
        detail = text;
    } else if let Some(text) = development_line(g, node, search, true)? {
        detail.push_str(&format!(" {text}"));
    }
    Ok(format!("{prefix}{detail}"))
}
pub fn technical_answer(
    g: &Game,
    node: usize,
    question: &str,
    lang: &str,
) -> Result<String, String> {
    let de = lang == "de";
    let n = &g.nodes[node];
    if !question.contains("Why")
        && g.human != "both"
        && n.fen.split_whitespace().nth(1) != Some(g.human.as_str())
    {
        return Ok(if de {
            "Warten wir auf den gegnerischen Zug. Dann prüfen wir deine Möglichkeiten."
        } else {
            "Let's wait for your opponent's move. Then we'll look at your options."
        }
        .into());
    }
    if question.contains("Why") {
        if node == 0 {
            return Ok(if de {
                "Es wurde noch kein Zug gespielt. Lass uns zuerst deine Möglichkeiten anschauen."
            } else {
                "No move has been played yet. Let's look at your options first."
            }
            .into());
        }
        let id = answer_node(g, node, question);
        if let Some(e) = &g.nodes[id].evidence {
            let explanation = e["explanation"].as_str().unwrap_or("");
            let alt = e["best_alternative"].as_str();
            let bad = matches!(
                e["label"].as_str(),
                Some("Inaccuracy" | "Mistake" | "Blunder" | "Miss")
            );
            if let Some(alt) = alt.filter(|_| {
                bad && !explanation.contains("stronger alternative")
                    && !explanation.contains("bessere Alternative")
            }) {
                return Ok(format!(
                    "{explanation} {} {alt}.",
                    if de {
                        "Vergleiche diesen Zug mit"
                    } else {
                        "Compare that with"
                    }
                ));
            }
            if bad {
                return Ok(explanation.to_owned());
            }
            let positive = matches!(
                e["label"].as_str(),
                Some("Best" | "Excellent" | "Good" | "Great" | "Brilliant" | "Book")
            );
            return Ok(format!(
                "{explanation} {}",
                if !positive {
                    if de {
                        "Die Einstufung dieses Zuges steht noch nicht fest."
                    } else {
                        "I haven't established this move's classification yet."
                    }
                } else if de {
                    "Dieser Zug erhält deine Möglichkeiten in der analysierten Variante."
                } else {
                    "That move keeps your chances in the line I checked."
                }
            ));
        }
    }
    let search: Search = serde_json::from_value(
        n.evidence
            .as_ref()
            .ok_or("Position analysis is still running")?["search"]
            .clone(),
    )
    .map_err(|e| e.to_string())?;
    let Some(line) = search.lines.first() else {
        return Ok(if de {
            "Diese Stellung hat keine legalen Züge mehr."
        } else {
            "There are no legal moves left in this position."
        }
        .into());
    };
    let mut p = position(&n.fen, g.chess960)?;
    let mut facts = vec![];
    for (i, text) in line.pv.iter().take(3).enumerate() {
        let m = text
            .parse::<UciMove>()
            .map_err(|e| e.to_string())?
            .to_move(&p)
            .map_err(|e| e.to_string())?;
        let san = line.san.get(i).unwrap_or(text);
        p.play_unchecked(m);
        if question.contains("threat") && i == 1 {
            let opening = &line.san[0];
            if let Move::Normal {
                to,
                capture: Some(c),
                ..
            } = m
            {
                return Ok(if de {
                    format!("Achte auf diese Möglichkeit: Nach {opening} kann {san} deinen {} auf {to} schlagen. Das ist eine konkrete Variante, keine erzwungene Zugfolge.",role(c,true))
                } else {
                    format!("Watch this idea: after {opening}, {san} can take your {} on {to}. That's one concrete continuation, rather than a forced sequence.",role(c,false))
                });
            }
            if p.is_check() {
                return Ok(if de {
                    format!("Nach {opening} ist {san} eine Schachmöglichkeit für deinen Gegner. Bereite eine Antwort darauf vor.")
                } else {
                    format!("After {opening}, your opponent can give check with {san}. Be ready to answer it.")
                });
            }
        }
        if question.contains("plan") && i == 0 {
            let detail = match m {
                Move::Normal {
                    role: r, from, to, ..
                } if [Role::Knight, Role::Bishop].contains(&r)
                    && ["1", "8"].contains(&&from.to_string()[1..]) =>
                {
                    if de {
                        format!(
                            "Damit bringst du deinen {} auf {to} ins Spiel.",
                            role(r, true)
                        )
                    } else {
                        format!("That brings your {} into play on {to}.", role(r, false))
                    }
                }
                Move::Normal {
                    capture: Some(c),
                    to,
                    ..
                } => {
                    if de {
                        format!("Dieser Zug schlägt {} auf {to}.", de_object(c, true))
                    } else {
                        format!("That captures the opponent's {} on {to}.", role(c, false))
                    }
                }
                Move::Castle { .. } => if de {
                    "Damit rochierst du."
                } else {
                    "That castles your king."
                }
                .into(),
                _ => {
                    if p.is_check() {
                        if de {
                            "Du beginnst mit Schach."
                        } else {
                            "You start with a check."
                        }
                        .into()
                    } else {
                        String::new()
                    }
                }
            };
            facts.push(if de {
                format!("Beginne mit {san}. {detail}")
            } else {
                format!("Start with {san}. {detail}")
            });
        }
    }
    let continuation = line
        .san
        .iter()
        .take(4)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if question.contains("threat") {
        return Ok(if de {
            format!("Ich habe hier noch keine konkrete taktische Drohung nachgewiesen. Prüfen wir diese Variante: {continuation}.")
        } else {
            format!("I haven't established a concrete tactical threat here yet. Let's examine this line: {continuation}.")
        });
    }
    Ok(format!(
        "{} {} {continuation}.",
        facts.join(" "),
        if de {
            "Eine mögliche Fortsetzung ist"
        } else {
            "One possible continuation is"
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(fen: &str, pv: &[&str], san: &[&str]) -> Game {
        let mut g = Game::new(fen.into(), "both".into(), 400, 0., 0., false).unwrap();
        g.nodes[0].evidence = Some(
            serde_json::json!({"search":{"lines":[{"depth":20,"cp":0,"mate":null,"wdl":null,"bound":false,"pv":pv,"san":san,"nodes":100,"nps":1000,"multipv":1}],"best":pv[0],"elapsed_ms":250.,"engine":"Stockfish 19"}}),
        );
        g
    }
    #[test]
    fn specific_plan_and_conditional_threat() {
        let g = fixture(
            "7k/8/8/8/3q4/2N5/8/1K6 w - - 0 1",
            &["c3d5", "d4d5"],
            &["Nd5", "Qxd5"],
        );
        let threat = technical_answer(&g, 0, "What is the threat?", "en").unwrap();
        assert!(threat.contains("after Nd5"));
        assert!(threat.contains("take your knight on d5"));
        assert!(threat.contains("one concrete continuation"));
        let plan = technical_answer(&g, 0, "What should my plan be?", "en").unwrap();
        assert!(plan.starts_with("Start with Nd5"));
        assert_ne!(plan, threat);
        assert!(technical_answer(&g, 0, "What is the threat?", "de")
            .unwrap()
            .contains("Springer"));
    }
    #[test]
    fn opponent_turn_never_recommends_opponents_move_as_yours() {
        let mut g = fixture(
            "7k/8/8/8/3q4/2N5/8/1K6 w - - 0 1",
            &["c3d5", "d4d5"],
            &["Nd5", "Qxd5"],
        );
        g.human = "b".into();
        for q in ["What is the threat?", "What should my plan be?"] {
            assert!(technical_answer(&g, 0, q, "en")
                .unwrap()
                .contains("wait for your opponent"));
        }
    }
    #[test]
    fn illegal_evidence_rejected() {
        let g = fixture(crate::game::START, &["e2e5"], &["e5"]);
        assert!(technical_answer(&g, 0, "What should my plan be?", "en").is_err());
    }
    #[test]
    fn why_uses_last_human_evidence() {
        let mut g = Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
        g.commit("e2e4", 0, false).unwrap();
        g.nodes[1].evidence =
            Some(serde_json::json!({"label":"Good","explanation":"Your pawn is in the center."}));
        g.commit("e7e5", 1, false).unwrap();
        assert!(technical_answer(&g, 2, "Why was that a mistake?", "en")
            .unwrap()
            .starts_with("Your pawn is in the center."));
    }
    #[test]
    fn root_and_unknown_grade_do_not_claim_a_good_move() {
        let mut g = fixture(crate::game::START, &["e2e4"], &["e4"]);
        assert!(technical_answer(&g, 0, "Why was that a mistake?", "en")
            .unwrap()
            .contains("No move has been played"));
        g.commit("e2e4", 0, false).unwrap();
        g.nodes[1].evidence = Some(serde_json::json!({"label":null,"explanation":"Checking."}));
        assert!(technical_answer(&g, 1, "Why was that a mistake?", "en")
            .unwrap()
            .contains("haven't established"));
    }
    #[test]
    fn opponent_error_is_players_opportunity() {
        let mut g = Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
        g.commit("e2e4", 0, false).unwrap();
        g.commit("e7e5", 1, false).unwrap();
        let search = Search {
            lines: vec![crate::engine::Line {
                pv: vec!["g1f3".into()],
                san: vec!["Nf3".into()],
                ..Default::default()
            }],
            best: "g1f3".into(),
            elapsed_ms: 250.,
            engine: "Stockfish19".into(),
        };
        let en = technical_friendly(&g, 2, Some("Mistake"), Some("c5"), &search, "en").unwrap();
        assert!(en.contains("Nf3"));
        assert!(en.contains("not been established"));
        assert!(!en.contains("Try c5 instead"));
        let de = technical_friendly(&g, 2, Some("Mistake"), Some("c5"), &search, "de").unwrap();
        assert!(de.contains("Nf3"));
        assert!(de.contains("nicht nachgewiesen"));
        assert!(!de.contains("stattdessen"));
    }
}

/// A slider attacks a blocker with a valuable opposing piece behind it. An absolute
/// pin is called a pin only when the rear piece is the king; relative rays are qualified.
pub fn verified_ray(
    g: &Game,
    node: usize,
    de: bool,
) -> Result<Option<(String, String, String)>, String> {
    let n = &g.nodes[node];
    let Some(parent) = n.parent else {
        return Ok(None);
    };
    let before = position(&g.nodes[parent].fen, g.chess960)?;
    let after = position(&n.fen, g.chess960)?;
    let m = n
        .uci
        .parse::<UciMove>()
        .map_err(|e| e.to_string())?
        .to_move(&before)
        .map_err(|e| e.to_string())?;
    let Move::Normal { role: r, to, .. } = m else {
        return Ok(None);
    };
    let directions: &[(i32, i32)] = match r {
        Role::Bishop => &[(1, 1), (-1, 1), (1, -1), (-1, -1)],
        Role::Rook => &[(1, 0), (-1, 0), (0, 1), (0, -1)],
        Role::Queen => &[
            (1, 1),
            (-1, 1),
            (1, -1),
            (-1, -1),
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
        ],
        _ => return Ok(None),
    };
    for &(dx, dy) in directions {
        let (mut x, mut y) = ((to as i32) % 8, (to as i32) / 8);
        let mut blocker: Option<(shakmaty::Square, Role)> = None;
        loop {
            x += dx;
            y += dy;
            if !(0..8).contains(&x) || !(0..8).contains(&y) {
                break;
            }
            let sq = shakmaty::Square::new((y * 8 + x) as u32);
            let Some(piece) = after.board().piece_at(sq) else {
                continue;
            };
            if piece.color == before.turn() {
                break;
            }
            if let Some((front, front_role)) = blocker {
                if piece.role == Role::King
                    || [Role::Queen, Role::Rook, Role::Bishop, Role::Knight].contains(&piece.role)
                        && front_role == Role::Pawn
                {
                    let text = if piece.role == Role::King {
                        if de {
                            format!(
                                "Der {} fesselt {} auf {front} an den König auf {sq}.",
                                role(r, true),
                                de_object(front_role, false)
                            )
                        } else {
                            format!(
                                "The {} pins the {} on {front} to the king on {sq}.",
                                role(r, false),
                                role(front_role, false)
                            )
                        }
                    } else if de {
                        format!("Der {} greift den Bauern auf {front} an. Dahinter steht {} {} auf {sq}: Verlässt der Bauer diese Linie, wird die Figur dahinter angegriffen.",role(r,true),if piece.role==Role::Queen {"die"} else {"der"},role(piece.role,true))
                    } else {
                        format!("The {} attacks the pawn on {front}. The {} on {sq} is behind it: moving the pawn off this line exposes that piece.",role(r,false),role(piece.role,false))
                    };
                    return Ok(Some((format!("{to}{sq}"), front.to_string(), text)));
                }
                break;
            } else {
                blocker = Some((sq, piece.role))
            }
        }
    }
    Ok(None)
}
fn reply_consequence(
    g: &Game,
    node: usize,
    search: &Search,
    de: bool,
) -> Result<Option<String>, String> {
    let Some(line) = search.lines.first() else {
        return Ok(None);
    };
    let Some(uci) = line.pv.first() else {
        return Ok(None);
    };
    let p = position(&g.nodes[node].fen, g.chess960)?;
    let m = uci
        .parse::<UciMove>()
        .map_err(|e| e.to_string())?
        .to_move(&p)
        .map_err(|e| e.to_string())?;
    Ok(
        if let Move::Normal {
            role: r,
            to,
            capture: Some(c),
            ..
        } = m
        {
            Some(if de {
                format!(
                    "Damit schlägt {} {} {} auf {to}.",
                    if r == Role::Queen { "die" } else { "der" },
                    role(r, true),
                    de_object(c, false)
                )
            } else {
                format!(
                    "That lets the {} capture the {} on {to}.",
                    role(r, false),
                    role(c, false)
                )
            })
        } else {
            None
        },
    )
}
fn development_line(
    g: &Game,
    node: usize,
    search: &Search,
    de: bool,
) -> Result<Option<String>, String> {
    let Some(line) = search.lines.first() else {
        return Ok(None);
    };
    let mut p = position(&g.nodes[node].fen, g.chess960)?;
    for (i, u) in line.pv.iter().take(2).enumerate() {
        let m = u
            .parse::<UciMove>()
            .map_err(|e| e.to_string())?
            .to_move(&p)
            .map_err(|e| e.to_string())?;
        if i == 1 {
            if let Move::Normal { role: r, from, .. } = m {
                if [Role::Knight, Role::Bishop].contains(&r)
                    && (from as u32 / 8 == 0 || from as u32 / 8 == 7)
                {
                    let first = line.san.first().cloned().unwrap_or_default();
                    let next = line.san.get(1).cloned().unwrap_or_default();
                    return Ok(Some(if de {
                        format!(
                            "Nach {first} bringt {next} den {} ins Spiel.",
                            role(r, true)
                        )
                    } else {
                        format!(
                            "After {first}, {next} brings the {} into play.",
                            role(r, false)
                        )
                    }));
                }
            }
        }
        p.play_unchecked(m);
    }
    Ok(None)
}

#[cfg(test)]
mod review_coach_tests {
    use super::*;
    #[test]
    fn ray_distinguishes_relative_and_absolute_pin() {
        let mut g =
            Game::new(crate::game::START.into(), "both".into(), 400, 0., 0., false).unwrap();
        for u in ["e2e4", "e7e5", "g1f3", "d8f6", "b1c3", "f8c5", "f1c4"] {
            g.commit(u, g.revision, false).unwrap();
        }
        let r = verified_ray(&g, g.live, true).unwrap().unwrap();
        assert_eq!(r.0, "c4g8");
        assert_eq!(r.1, "f7");
        assert!(r.2.contains("Verlässt"));
        assert!(!r.2.contains("fesselt"));
        let mut absolute = Game::new(
            "6k1/5p2/8/8/8/1B6/8/7K w - - 0 1".into(),
            "both".into(),
            400,
            0.,
            0.,
            false,
        )
        .unwrap();
        absolute.commit("b3c4", 0, false).unwrap();
        let a = verified_ray(&absolute, absolute.live, true)
            .unwrap()
            .unwrap();
        assert!(a.2.contains("fesselt"));
        assert!(!absolute
            .snapshot(None)
            .unwrap()
            .legal
            .contains(&"f7f6".to_owned()));
    }
    #[test]
    fn development_text_uses_actual_legal_two_ply_line() {
        let mut g = Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
        g.commit("e2e4", 0, false).unwrap();
        let s = Search {
            lines: vec![crate::engine::Line {
                pv: vec!["e7e5".into(), "g1f3".into()],
                san: vec!["e5".into(), "Nf3".into()],
                ..Default::default()
            }],
            best: "e7e5".into(),
            elapsed_ms: 0.,
            engine: "test".into(),
        };
        assert!(technical_friendly(&g, 1, Some("Book"), None, &s, "de")
            .unwrap()
            .contains("den Springer ins Spiel"));
        let invalid = Search {
            lines: vec![crate::engine::Line {
                pv: vec!["a1a8".into()],
                ..Default::default()
            }],
            ..s
        };
        assert!(development_line(&g, 1, &invalid, true).is_err());
    }
}

/// Explain the actual consequence, including recaptures, rather than treating
/// every capture in a PV as a material win. Every displayed SAN is rebuilt
/// from the legal position; provider prose never decides a chess fact.
pub fn mistake_reason(
    g: &Game,
    node: usize,
    alt: Option<&str>,
    search: &Search,
    lang: &str,
) -> Result<String, String> {
    let de = lang == "de";
    let n = &g.nodes[node];
    let Some(parent) = n.parent else {
        return Ok(String::new());
    };
    let before = position(&g.nodes[parent].fen, g.chess960)?;
    let played = n
        .uci
        .parse::<UciMove>()
        .map_err(|e| e.to_string())?
        .to_move(&before)
        .map_err(|e| e.to_string())?;
    let mover = before.turn();
    let mut p = position(&n.fen, g.chess960)?;
    let mut san = vec![];
    let mut first_capture = None;
    let mut captures = vec![];
    let mut gain = 0i32;
    let mut mate = false;
    let value = |r: Role| match r {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 0,
    };
    if let Some(line) = search.lines.first() {
        for u in line.pv.iter().take(6) {
            let m = u
                .parse::<UciMove>()
                .map_err(|e| e.to_string())?
                .to_move(&p)
                .map_err(|e| e.to_string())?;
            let notation = shakmaty::san::SanPlus::from_move(p.clone(), m).to_string();
            let capture = match m {
                Move::Normal {
                    to,
                    capture: Some(r),
                    ..
                } => Some((to, r)),
                Move::EnPassant { to, .. } => {
                    Some((shakmaty::Square::new((to as u32) ^ 8), Role::Pawn))
                }
                _ => None,
            };
            if let Some((to, r)) = capture {
                captures.push((san.len(), to, r, m.role()));
                gain += if p.turn() != mover {
                    value(r)
                } else {
                    -value(r)
                };
                if first_capture.is_none() && p.turn() != mover {
                    first_capture = Some((notation.clone(), to, r, san.len()));
                }
            }
            if let Move::Normal {
                promotion: Some(r), ..
            } = m
            {
                gain += if p.turn() != mover {
                    value(r) - 1
                } else {
                    -(value(r) - 1)
                };
            }
            san.push(notation);
            p.play_unchecked(m);
            if p.is_checkmate() {
                mate = true;
                break;
            }
        }
    }
    let side = if mover == shakmaty::Color::White {
        if de {
            "Weiß"
        } else {
            "White"
        }
    } else if de {
        "Schwarz"
    } else {
        "Black"
    };
    let opponent = if mover == shakmaty::Color::White {
        if de {
            "Schwarz"
        } else {
            "Black"
        }
    } else if de {
        "Weiß"
    } else {
        "White"
    };
    let exchange = if captures.len() == 3 {
        let (a, b, c) = (captures[0], captures[1], captures[2]);
        a.0 == 0
            && b.0 == 1
            && c.0 == 2
            && a.1 == b.1
            && b.1 == c.1
            && value(a.2) == value(b.2)
            && c.2 == Role::Pawn
            && gain == 1
    } else {
        false
    };
    let mut text = if exchange {
        let a = captures[0];
        if de {
            format!("{} erlaubt {} {} {}: {opponent} tauscht {} gegen {} und schlägt danach mit {} den Bauern auf {}.",n.san,san[0],san[1],san[2],de_object(a.3,false),de_object(a.2,false),san[2],a.1)
        } else {
            format!("{} allows {} {} {}: {opponent} trades the {} for the {}, then picks up the pawn on {} with {}.",n.san,san[0],san[1],san[2],role(a.3,false),role(a.2,false),a.1,san[2])
        }
    } else if mate && p.turn() == mover {
        if de {
            format!("{} erlaubt ein Matt: {}. In dieser geprüften Variante hat der König von {side} am Ende keinen legalen Ausweg.",n.san,san.join(" "))
        } else {
            format!("{} allows checkmate: {}. At the end of this checked line, {side}'s king has no legal escape.",n.san,san.join(" "))
        }
    } else if let Some((reply, to, r, ply)) = first_capture {
        let direct = ply == 0 && played.to() == to;
        let cause = if direct {
            if de {
                format!(
                    "{} stellt {} auf {to}, wo {reply} die Figur schlägt.",
                    n.san,
                    de_object(r, false)
                )
            } else {
                format!(
                    "{} puts the {} on {to}, where {reply} captures it.",
                    n.san,
                    role(r, false)
                )
            }
        } else if de {
            format!(
                "Nach {} kann {opponent} mit {reply} {} auf {to} schlagen.",
                n.san,
                de_object(r, false)
            )
        } else {
            format!(
                "After {}, {opponent} can play {reply} to capture the {} on {to}.",
                n.san,
                role(r, false)
            )
        };
        if gain > 0 {
            if de {
                format!("{cause} In der geprüften Fortsetzung {} gewinnt {opponent} nach den Rückschlägen netto {gain} Materialpunkt{} (Bauer = 1).",san.join(" "),if gain==1{""}else{"e"})
            } else {
                format!("{cause} In the checked continuation {}, {opponent} gains a net {gain} material point{} after the replies (pawn = 1).",san.join(" "),if gain==1{""}else{"s"})
            }
        } else {
            if de {
                format!("{cause} Prüfe aber auch die Rückschläge: {}. Diese Variante belegt keinen Nettogewinn an Material.",san.join(" "))
            } else {
                format!("{cause} Check the recaptures too: {}. This line does not establish a net material gain.",san.join(" "))
            }
        }
    } else if de {
        format!("Nach {} ist {} die geprüfte Fortsetzung. Ein konkreter Materialverlust ist darin bisher nicht nachgewiesen.",n.san,if san.is_empty(){"noch keine Antwort verfügbar".into()}else{san.join(" ")})
    } else {
        format!("After {}, the checked continuation is {}. A specific material loss has not been established in this line yet.",n.san,if san.is_empty(){"not available yet".into()}else{san.join(" ")})
    };
    if let Some(a) = alt {
        if de {
            text.push_str(&format!(" Für {side} war {a} die bessere Alternative."));
        } else {
            text.push_str(&format!(" {side} had the stronger alternative {a}."));
        }
    }
    Ok(text)
}

#[cfg(test)]
mod reason_tests {
    use super::*;
    fn search(pv: &[&str]) -> Search {
        Search {
            lines: vec![crate::engine::Line {
                pv: pv.iter().map(|s| s.to_string()).collect(),
                depth: 20,
                ..Default::default()
            }],
            best: pv.first().unwrap_or(&"").to_string(),
            elapsed_ms: 0.,
            engine: "fixture".into(),
        }
    }
    #[test]
    fn own_and_opponent_hanging_piece_are_specific() {
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
        let s = search(&["d4d5"]);
        for human in ["w", "b"] {
            g.human = human.into();
            let en = technical_friendly(&g, 1, Some("Blunder"), None, &s, "en").unwrap();
            assert!(en.contains("knight on d5"), "{en}");
            assert!(en.contains("Qxd5"));
            assert!(en.contains("net 3"));
            assert!(!en.contains("given you an opportunity"));
            let de = technical_friendly(&g, 1, Some("Blunder"), None, &s, "de").unwrap();
            assert!(de.contains("Springer auf d5"));
            assert!(de.contains("netto 3"));
        }
    }
    #[test]
    fn recapture_is_not_a_claimed_material_win() {
        let mut g = Game::new(
            "7k/8/8/4p3/3P4/8/8/K7 w - - 0 1".into(),
            "both".into(),
            400,
            0.,
            0.,
            false,
        )
        .unwrap();
        g.commit("a1b1", 0, false).unwrap();
        let t = mistake_reason(&g, 1, None, &search(&["e5d4"]), "en").unwrap();
        assert!(t.contains("pawn on d4"));
        let mut g = Game::new(
            "7k/8/8/4p3/3P4/2P5/8/K7 w - - 0 1".into(),
            "both".into(),
            400,
            0.,
            0.,
            false,
        )
        .unwrap();
        g.commit("a1b1", 0, false).unwrap();
        let t = mistake_reason(&g, 1, None, &search(&["e5d4", "c3d4"]), "en").unwrap();
        assert!(t.contains("does not establish a net material gain"), "{t}");
        assert!(!t.contains("gains a net"));
    }
    #[test]
    fn mate_is_replayed_and_illegal_claims_rejected() {
        let mut g =
            Game::new(crate::game::START.into(), "both".into(), 400, 0., 0., false).unwrap();
        for u in ["f2f3", "e7e5", "g2g4"] {
            g.commit(u, g.revision, false).unwrap();
        }
        let t = mistake_reason(&g, 3, None, &search(&["d8h4"]), "en").unwrap();
        assert!(t.contains("allows checkmate"));
        assert!(t.contains("Qh4#"));
        assert!(mistake_reason(&g, 3, None, &search(&["a1a8"]), "en").is_err());
    }
}

#[cfg(test)]
mod screenshot_reason_test {
    use super::*;
    #[test]
    fn explains_exchange_then_extra_pawn() {
        let mut g = Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
        for u in [
            "e2e4", "c7c6", "g1f3", "d8c7", "b1c3", "h7h6", "f1c4", "g8f6", "e4e5", "f6d5",
        ] {
            g.commit(u, g.revision, false).unwrap();
        }
        let s = Search {
            lines: vec![crate::engine::Line {
                pv: ["c4d5", "c6d5", "c3d5"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                ..Default::default()
            }],
            best: "c4d5".into(),
            elapsed_ms: 0.,
            engine: "fixture".into(),
        };
        let en = mistake_reason(&g, g.live, None, &s, "en").unwrap();
        assert!(en.contains("trades the bishop for the knight"));
        assert!(en.contains("pawn on d5 with Nxd5"));
        let de = mistake_reason(&g, g.live, None, &s, "de").unwrap();
        assert!(de.contains("tauscht den Läufer gegen den Springer"));
        assert!(de.contains("mit Nxd5 den Bauern auf d5"));
    }
}

/// Live Why asks about the last human decision; guided review always names
/// its exact selected node. Providers must use the same node as the answer.
pub fn answer_node(g: &Game, node: usize, question: &str) -> usize {
    let mut id = node;
    if question.contains("Why") && node == g.live && g.human != "both" {
        while let Some(parent) = g.nodes[id].parent {
            if g.nodes[parent].fen.split_whitespace().nth(1) == Some(g.human.as_str()) {
                break;
            }
            id = parent;
        }
    }
    id
}
#[cfg(test)]
mod provider_node_tests {
    use super::*;
    #[test]
    fn provider_facts_and_live_why_share_decision_node() {
        let mut g = Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
        g.commit("e2e4", 0, false).unwrap();
        g.commit("e7e5", 1, false).unwrap();
        assert_eq!(answer_node(&g, 2, "Why was that a mistake?"), 1);
        assert_eq!(answer_node(&g, 2, "Review this move"), 2);
        g.human = "b".into();
        assert_eq!(answer_node(&g, 2, "Why was that a mistake?"), 2);
        g.human = "both".into();
        assert_eq!(answer_node(&g, 2, "Why was that a mistake?"), 2);
    }
}

#[cfg(test)]
mod ep_reason_test {
    use super::*;
    #[test]
    fn en_passant_names_removed_pawns_square_not_landing_square() {
        let mut g = Game::new(
            "7k/3p4/8/4P3/8/8/8/K7 b - - 0 1".into(),
            "both".into(),
            400,
            0.,
            0.,
            false,
        )
        .unwrap();
        g.commit("d7d5", 0, false).unwrap();
        let search = Search {
            lines: vec![crate::engine::Line {
                pv: vec!["e5d6".into()],
                ..Default::default()
            }],
            best: "e5d6".into(),
            elapsed_ms: 0.,
            engine: "fixture".into(),
        };
        let en = mistake_reason(&g, 1, None, &search, "en").unwrap();
        assert!(en.contains("pawn on d5"));
        assert!(en.contains("exd6"));
        assert!(!en.contains("pawn on d6"));
        let de = mistake_reason(&g, 1, None, &search, "de").unwrap();
        assert!(de.contains("Bauern auf d5"));
        assert!(!de.contains("Bauern auf d6"));
    }
}

// Everyday feedback stays conversational; technical helpers remain for Explain.
pub fn friendly(
    g: &Game,
    node: usize,
    label: Option<&str>,
    alt: Option<&str>,
    search: &Search,
    lang: &str,
) -> Result<String, String> {
    crate::coach_voice::message_with_alternative(g, node, label, search, lang, alt)
}
pub fn answer(g: &Game, node: usize, question: &str, lang: &str) -> Result<String, String> {
    crate::coach_voice::answer(g, node, question, lang)
}
