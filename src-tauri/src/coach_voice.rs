//! Conversational coaching. Every theme is derived from a legal move or a
//! replayed engine continuation; notation remains in the analysis UI.
use crate::{
    engine::Search,
    game::{position, Game},
};
use shakmaty::{uci::UciMove, Color, Move, Position, FromSetup, Role, Square};

fn name(r: Role, de: bool) -> &'static str {
    match (r, de) {
        (Role::Pawn, true) => "Bauer",
        (Role::Knight, true) => "Springer",
        (Role::Bishop, true) => "Läufer",
        (Role::Rook, true) => "Turm",
        (Role::Queen, true) => "Dame",
        (Role::King, true) => "König",
        (Role::Pawn, false) => "pawn",
        (Role::Knight, false) => "knight",
        (Role::Bishop, false) => "bishop",
        (Role::Rook, false) => "rook",
        (Role::Queen, false) => "queen",
        (Role::King, false) => "king",
    }
}
fn object(r: Role, de: bool, owner: &str) -> String {
    if !de {
        return format!("{owner} {}", name(r, false));
    }
    let ending = if r == Role::Queen { "e" } else { "en" };
    let noun = if r == Role::Pawn {
        "Bauern"
    } else {
        name(r, true)
    };
    format!("{owner}{ending} {noun}")
}
fn value(r: Role) -> i32 {
    match r {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 0,
    }
}
fn capture(m: Move) -> Option<(Square, Role)> {
    match m {
        Move::Normal {
            to,
            capture: Some(r),
            ..
        } => Some((to, r)),
        Move::EnPassant { to, .. } => Some((Square::new(to as u32 ^ 8), Role::Pawn)),
        _ => None,
    }
}
fn legal_capture(p: &shakmaty::Chess, sq: Square) -> bool {
    p.legal_moves()
        .iter()
        .any(|m| capture(*m).is_some_and(|(to, _)| to == sq))
}

// Legal captures from the moved piece, with turn changed only when that is a
// valid position. Pinned pieces never manufacture a legal tactical threat.
fn attack_target(p: &shakmaty::Chess, from: Square) -> Option<Role> {
    let own = p.clone().swap_turn().ok()?;
    own.legal_moves()
        .iter()
        .filter(|m| m.from() == Some(from))
        .filter_map(|m| capture(*m).map(|(_, r)| r))
        .max_by_key(|r| value(*r))
}
fn centre_control(p: &shakmaty::Chess, from: Square) -> bool {
    let Some(piece) = p.board().piece_at(from) else {
        return false;
    };
    let attacks = shakmaty::attacks::attacks(from, piece, p.board().occupied());
    [Square::D4, Square::E4, Square::D5, Square::E5]
        .iter()
        .any(|sq| attacks.contains(*sq))
}

// Replace the friendly pawn with an opposing pawn, then require a legal capture.
// This excludes pinned defenders and handles pawn diagonals rather than pushes.
fn legal_defenders(p: &shakmaty::Chess, target: Square, side: Color) -> Vec<Square> {
    let mut setup = p.to_setup(shakmaty::EnPassantMode::Legal);
    setup.board.set_piece_at(target, shakmaty::Piece { color:side.other(), role:Role::Pawn });
    setup.turn = side;
    setup.ep_square = None;
    let Ok(test) = shakmaty::Chess::from_setup(setup, p.castles().mode()) else { return vec![]; };
    test.legal_moves().iter().filter(|m| capture(**m).is_some_and(|(sq,_)|sq==target))
        .filter_map(|m| m.from()).collect()
}
fn new_pawn_defense(before: &shakmaty::Chess, after: &shakmaty::Chess, from:Square, to:Square) -> bool {
    after.board().pawns().into_iter().any(|sq|
        after.board().color_at(sq)==Some(before.turn())
        && legal_defenders(before,sq,before.turn()).is_empty()
        && legal_defenders(after,sq,before.turn()).contains(&to)
        && !legal_defenders(before,sq,before.turn()).contains(&from))
}
fn bishop_mobility(p:&shakmaty::Chess, side:Color)->usize {
    let own=if p.turn()==side {Some(p.clone())}else{p.clone().swap_turn().ok()};
    own.map(|p|p.legal_moves().iter().filter(|m|m.role()==Role::Bishop).count()).unwrap_or(0)
}
fn pawn_pressure_on_pin(after:&shakmaty::Chess,s:&Search)->Option<Role> {
    let text=s.lines.first()?.pv.first()?;
    let reply=text.parse::<UciMove>().ok()?.to_move(after).ok()?;
    if reply.role()!=Role::Pawn || capture(reply).is_some() {return None;}
    let side=after.turn().other();
    let king=after.board().king_of(side)?;
    let mut next=after.clone();next.play_unchecked(reply);
    let attacks=shakmaty::attacks::attacks(reply.to(),shakmaty::Piece{color:after.turn(),role:Role::Pawn},next.board().occupied());
    for target in (attacks & next.board().by_color(side)).into_iter() {
        let without=after.board().occupied().without(target);
        let revealed=after.board().attacks_to(king,after.turn(),without)
            .without(after.board().attacks_to(king,after.turn(),after.board().occupied()));
        if revealed.into_iter().any(|sq|matches!(after.board().role_at(sq),Some(Role::Bishop|Role::Rook|Role::Queen))) {
            return after.board().role_at(target);
        }
    }
    None
}
fn exchange_to_rim(after:&shakmaty::Chess,s:&Search)->bool {
    let Some(line)=s.lines.first() else{return false};
    let mut p=after.clone();let mut target=None;
    for (i,u) in line.pv.iter().take(2).enumerate() {
        let Some(m)=u.parse::<UciMove>().ok().and_then(|u|u.to_move(&p).ok()) else{return false};
        if i==0 {target=capture(m).map(|(sq,_)|sq);if target.is_none(){return false;}}
        else {return m.role()==Role::Knight && capture(m).is_some_and(|(sq,_)|Some(sq)==target)
            && matches!(m.to().file(),shakmaty::File::A|shakmaty::File::H);}
        p.play_unchecked(m);
    }
    false
}

#[derive(Default)]
struct Consequence {
    gain: i32,
    victim: Option<Role>,
    mate: bool,
    recapture: bool,
    promotion: bool,
}
fn consequence(g: &Game, node: usize, s: &Search) -> Result<Consequence, String> {
    let mut p = position(&g.nodes[node].fen, g.chess960)?;
    let mover = p.turn().other();
    let mut c = Consequence::default();
    let mut first_attacker_square: Option<Square> = None;
    if let Some(line) = s.lines.first() {
        for u in line.pv.iter().take(12) {
            let m = u
                .parse::<UciMove>()
                .map_err(|e| e.to_string())?
                .to_move(&p)
                .map_err(|e| e.to_string())?;
            if let Some((_, r)) = capture(m) {
                if p.turn() != mover {
                    c.gain += value(r);
                    if c.victim.is_none() {
                        c.victim = Some(r);
                        first_attacker_square=Some(m.to());
                    }
                } else {
                    c.gain -= value(r);
                    if first_attacker_square==Some(m.to()) {
                        c.recapture=true;
                        first_attacker_square=None;
                    }
                }
            }
            if p.turn()!=mover && m.from()==first_attacker_square {
                first_attacker_square=Some(m.to());
            }
            if let Move::Normal {
                promotion: Some(r), ..
            } = m
            {
                c.promotion = true;
                c.gain += if p.turn() != mover {
                    value(r) - 1
                } else {
                    -(value(r) - 1)
                }
            }
            p.play_unchecked(m);
            if p.is_checkmate() {
                c.mate = p.turn() == mover;
                break;
            }
        }
    }
    Ok(c)
}
fn praise(label: Option<&str>, de: bool) -> &'static str {
    match (label, de) {
        (Some("Best" | "Great"), true) => "Stark gespielt! ",
        (Some("Excellent"), true) => "Sehr schön! ",
        (Some("Good"), true) => "Guter Zug. ",
        (Some("Brilliant"), true) => "Großartig gefunden! ",
        (Some("Best" | "Great"), false) => "Great move! ",
        (Some("Excellent"), false) => "Nicely done! ",
        (Some("Good"), false) => "Good move. ",
        (Some("Brilliant"), false) => "Beautifully found! ",
        _ => "",
    }
}
// Describe a legal move's visible purpose, without treating an engine score as
// proof of a tactic. This also gives quiet positional alternatives useful words.
fn move_purpose(before: &shakmaty::Chess, m: Move, de: bool, yours: bool) -> String {
    let mut after = before.clone();
    after.play_unchecked(m);
    let owner = if yours { "your" } else { "my" };
    let de_owner = if yours { "dein" } else { "mein" };
    if let Some((_, victim)) = capture(m) {
        return if de {
            format!(
                "{} schlagen",
                object(victim, true, if yours { "mein" } else { "dein" })
            )
        } else {
            format!(
                "capture {}",
                object(victim, false, if yours { "my" } else { "your" })
            )
        };
    }
    if m.is_castle() {
        return if de {
            "rochieren und den Turm ins Spiel bringen"
        } else {
            "castle and bring the rook into play"
        }
        .into();
    }
    if let Move::Normal {
        role,
        from,
        to,
        promotion,
        ..
    } = m
    {
        if let Some(r) = promotion {
            return if de {
                format!(
                    "den Bauern in {} {} umwandeln",
                    if r == Role::Queen { "eine" } else { "einen" },
                    name(r, true)
                )
            } else {
                format!("promote the pawn to a {}", name(r, false))
            };
        }
        if before
            .clone()
            .swap_turn()
            .ok()
            .is_some_and(|p| legal_capture(&p, from))
            && !legal_capture(&after, to)
        {
            return if de {
                format!(
                    "{} aus dem direkten Angriff bringen",
                    object(role, true, de_owner)
                )
            } else {
                format!(
                    "move {owner} {} out of the immediate attack",
                    name(role, false)
                )
            };
        }
        if after.is_check() {
            return if de {
                "den gegnerischen König mit Schach angreifen"
            } else {
                "give check and make the king respond"
            }
            .into();
        }
        if let Some(target) = attack_target(&after, to) {
            return if de {
                format!(
                    "mit {} {} angreifen",
                    if yours {
                        "deiner Figur"
                    } else {
                        "meiner Figur"
                    },
                    object(target, true, if yours { "mein" } else { "dein" })
                )
            } else {
                format!(
                    "attack {} with {owner} {}",
                    object(target, false, if yours { "my" } else { "your" }),
                    name(role, false)
                )
            };
        }
        if matches!(role, Role::Knight | Role::Bishop) && from.rank() == before.turn().backrank() {
            let centre = centre_control(&after, to);
            return if de {
                format!(
                    "{} ins Spiel bringen{}",
                    object(role, true, de_owner),
                    if centre {
                        " und das Zentrum angreifen"
                    } else {
                        ""
                    }
                )
            } else {
                format!(
                    "bring {owner} {} into play{}",
                    name(role, false),
                    if centre {
                        " and control central squares"
                    } else {
                        ""
                    }
                )
            };
        }
        if centre_control(&after, to) && !centre_control(before, from) {
            return if de {
                format!(
                    "mit {} ins Zentrum eingreifen",
                    if yours {
                        "deiner Figur"
                    } else {
                        "meiner Figur"
                    }
                )
            } else {
                format!(
                    "use {owner} {} to control central squares",
                    name(role, false)
                )
            };
        }
        if role == Role::Rook
            && !after
                .board()
                .pawns()
                .into_iter()
                .any(|sq| sq.file() == to.file())
        {
            return if de {
                format!(
                    "{} auf eine offene Linie stellen",
                    object(role, true, de_owner)
                )
            } else {
                format!("put {owner} rook on an open file")
            };
        }
        if role == Role::Pawn && [Square::D4, Square::E4, Square::D5, Square::E5].contains(&to) {
            return if de {
                "mit dem Bauern Raum im Zentrum gewinnen"
            } else {
                "take space in the centre with the pawn"
            }
            .into();
        }
        return if de {
            format!("{} neu positionieren", object(role, true, de_owner))
        } else {
            format!("reposition {owner} {}", name(role, false))
        };
    }
    if de {
        "die Figuren besser aufstellen"
    } else {
        "improve the piece placement"
    }
    .into()
}

fn quiet_reason(
    g: &Game,
    node: usize,
    before: &shakmaty::Chess,
    after: &shakmaty::Chess,
    played: Move,
    s: &Search,
    alt: Option<&str>,
    de: bool,
    yours: bool,
) -> String {
    let n = &g.nodes[node];
    let stored = n
        .evidence
        .as_ref()
        .and_then(|e| e["best_alternative"].as_str());
    let parent_best = n
        .parent
        .and_then(|id| g.nodes[id].evidence.as_ref())
        .and_then(|e| e["search"]["best"].as_str());
    let candidate = alt.or(stored).or(parent_best);
    let alternative = candidate.and_then(|u| {
        before.legal_moves().into_iter().find(|m| {
            *m != played
                && (shakmaty::san::SanPlus::from_move(before.clone(), *m).to_string() == u
                    || u.parse::<UciMove>()
                        .ok()
                        .and_then(|u| u.to_move(before).ok())
                        == Some(*m))
        })
    });
    if let Some(m) = alternative {
        let purpose = move_purpose(before, m, de, yours);
        let lead = if de {
            if yours {
                "Du hättest stattdessen"
            } else {
                "Ich hätte stattdessen"
            }
        } else if yours {
            "The stronger option was to"
        } else {
            "I should have used this move to"
        };
        // A capture, legal threat, escape, central control or development names a
        // concrete opportunity. Generic relocation needs the checked reply too.
        if !purpose.contains("reposition") && !purpose.contains("neu positionieren") {
            let mut alternative_position = before.clone();
            alternative_position.play_unchecked(m);
            let reason = if let Move::Normal {
                role: Role::Pawn,
                from,
                to,
                ..
            } = played
            {
                if !centre_control(after, to)
                    && !centre_control(before, from)
                    && centre_control(&alternative_position, m.to())
                {
                    match (yours, de) {
                        (true, true) => {
                            "Dieser Bauernzug verstärkt deine Kontrolle im Zentrum nicht. "
                        }
                        (false, true) => {
                            "Mein Bauernzug verstärkt meine Kontrolle im Zentrum nicht. "
                        }
                        (true, false) => "This pawn move doesn't add control of central squares. ",
                        (false, false) => "My pawn move doesn't add control of central squares. ",
                    }
                } else {
                    ""
                }
            } else {
                ""
            };
            return if de {
                format!("{reason}{lead} {purpose}{}", " sollen.")
            } else {
                format!("{reason}{lead} {purpose}.")
            };
        }
    }
    if let Some(u) = s.lines.first().and_then(|l| l.pv.first()) {
        if let Some(reply) = u
            .parse::<UciMove>()
            .ok()
            .and_then(|u| u.to_move(after).ok())
        {
            let purpose = move_purpose(after, reply, de, !yours);
            return if de {
                format!(
                    "{} jetzt {purpose}. Prüfe diese Antwort, bevor du deinen Plan fortsetzt.",
                    if yours {
                        "Dein Gegner kann"
                    } else {
                        "Du kannst"
                    }
                )
            } else {
                format!(
                    "{} now {purpose}. Check this reply before continuing your plan.",
                    if yours {
                        "Your opponent can"
                    } else {
                        "You can"
                    }
                )
            };
        }
    }
    // With no legal continuation supplied, describe only the actual move.
    // Never invent a material loss, pin or mating threat to justify the grade.
    let purpose = move_purpose(before, played, de, yours);
    if de {
        format!("Dieser Zug dient dazu, {purpose}. Achte vor dem nächsten Zug auf Angriffe auf deine Figuren und deinen König.")
    } else {
        format!("This move aims to {purpose}. Before your next move, check attacks on your pieces and king.")
    }
}

pub fn message(
    g: &Game,
    node: usize,
    label: Option<&str>,
    s: &Search,
    lang: &str,
) -> Result<String, String> {
    message_with_alternative(g, node, label, s, lang, None)
}
pub fn message_with_alternative(
    g: &Game,
    node: usize,
    label: Option<&str>,
    s: &Search,
    lang: &str,
    alternative: Option<&str>,
) -> Result<String, String> {
    let de = lang == "de";
    let n = g.nodes.get(node).ok_or("Unknown coach position")?;
    let Some(parent) = n.parent else {
        return Ok(if de {
            "Schauen wir uns deine Möglichkeiten an. Du bist am Zug."
        } else {
            "Let's look at your options. It's your turn."
        }
        .into());
    };
    let before = position(&g.nodes[parent].fen, g.chess960)?;
    let after = position(&n.fen, g.chess960)?;
    let m = n
        .uci
        .parse::<UciMove>()
        .map_err(|e| e.to_string())?
        .to_move(&before)
        .map_err(|e| e.to_string())?;
    // Manual two-player games retain the default White coaching perspective.
    // Board control of both colors must not make Black moves become "your" moves.
    let yours = (before.turn() == Color::White) == (g.human != "b");
    if label==Some("Brilliant") && forced_queen_knight_mate(&before,&after,&m,s) {
        return Ok(match(yours,de) {
            (true,false)=>"Beautiful queen sacrifice! The forced capture lets your knight deliver checkmate.",
            (false,false)=>"My queen sacrifice forces the capture, then my knight delivers checkmate.",
            (true,true)=>"Starkes Damenopfer! Der erzwungene Rückschlag erlaubt deinem Springer, mattzusetzen.",
            (false,true)=>"Mein Damenopfer erzwingt den Rückschlag. Danach setzt mein Springer matt.",
        }.into());
    }
    if after.is_checkmate() {
        return Ok(match (yours, de) {
            (true, true) => "Schachmatt! Gut gespielt. Du hast die Partie gewonnen.",
            (false, true) => "Schachmatt. Lass uns gemeinsam schauen, was du daraus lernen kannst.",
            (true, false) => "Checkmate! Well played. You've won the game.",
            (false, false) => "Checkmate. Let's look at what you can learn from this game.",
        }
        .into());
    }
    if matches!(label, Some("Inaccuracy" | "Mistake" | "Blunder" | "Miss")) {
        if let Some(target)=pawn_pressure_on_pin(&after,s) {
            return Ok(if de {format!("{} {} ist an den König gefesselt. {} den Druck mit einem Bauernangriff erhöhen.",
                if yours {"Dein"}else{"Mein"},name(target,true),if yours{"Dein Gegner kann"}else{"Du kannst"})}
                else {format!("{} {} is pinned to the king. {} increase the pressure with a pawn attack.",
                    if yours{"Your"}else{"My"},name(target,false),if yours{"Your opponent can"}else{"You can"})});
        }
        let c = consequence(g, node, s)?;
        if c.mate {
            return Ok(match (yours, de) {
                (true, true) => "Vorsicht! Dieser Zug öffnet deinem Gegner einen Weg zum Matt.",
                (false, true) => "Mein König ist in Gefahr. Du hast jetzt eine Mattchance.",
                (true, false) => "Careful! This gives your opponent a path to checkmate.",
                (false, false) => "My king is in danger. You have a chance to find checkmate now.",
            }
            .into());
        }
        if c.promotion && c.gain > 0 {
            return Ok(match(yours,de){(true,true)=>"Vorsicht: In der geprüften Fortsetzung kann dein Gegner einen Bauern umwandeln und Material gewinnen.",(false,true)=>"In der geprüften Fortsetzung kannst du einen Bauern umwandeln und Material gewinnen.",(true,false)=>"Careful: in the checked line, your opponent can promote a pawn and gain material.",(false,false)=>"In the checked line, you can promote a pawn and gain material."}.into());
        }
        if c.gain > 0 {
            if c.recapture && c.victim.is_none_or(|r| c.gain < (value(r)+1)/2) {
                return Ok(if de {
                    format!(
                        "{} In der geprüften Fortsetzung bleibt {} nach den Rückschlägen {} mehr.",
                        if yours {
                            "Achte auf den Tausch."
                        } else {
                            "Dieser Tausch bietet dir eine Chance."
                        },
                        if yours { "deinem Gegner" } else { "dir" },
                        if c.gain == 1 { "ein Bauer" } else { "Material" }
                    )
                } else {
                    format!(
                        "Watch the trade. In the checked line, {} {} after the recaptures.",
                        if yours {
                            "your opponent comes out"
                        } else {
                            "you come out"
                        },
                        if c.gain == 1 {
                            "a pawn ahead"
                        } else {
                            "ahead in material"
                        }
                    )
                });
            }
            if let Some(r) = c.victim {
                return Ok(if de {
                    if yours {
                        format!(
                            "Pass auf! Dein Gegner kann {} schlagen und Material gewinnen.",
                            object(r, true, "dein")
                        )
                    } else {
                        format!("Ich habe dir eine Chance gegeben. Du kannst {} schlagen und Material gewinnen.",object(r,true,"mein"))
                    }
                } else if yours {
                    format!(
                        "Careful! Your opponent can capture {} and win material.",
                        object(r, false, "your")
                    )
                } else {
                    format!(
                        "I've given you a chance. You can capture {} and win material.",
                        object(r, false, "my")
                    )
                });
            }
        }
        if exchange_to_rim(&after,s) {
            return Ok(match (yours,de) {
                (true,true)=>"Du erlaubst einen Tausch, der deinen Springer an den Rand des Bretts zieht.",
                (false,true)=>"Ich habe einen Tausch erlaubt, der meinen Springer an den Rand des Bretts zieht.",
                (true,false)=>"You've allowed an exchange that draws your knight to the edge of the board.",
                (false,false)=>"I've allowed an exchange that draws my knight to the edge of the board.",
            }.into());
        }
        return Ok(quiet_reason(
            g,
            node,
            &before,
            &after,
            m,
            s,
            alternative,
            de,
            yours,
        ));
    }
    let prefix = if yours { praise(label, de) } else { "" };
    let text = if let Move::Normal {
        promotion: Some(r), ..
    } = m
    {
        if de {
            format!(
                "{} Bauer wird zu {} {}.",
                if yours { "Dein" } else { "Mein" },
                if r == Role::Queen { "einer" } else { "einem" },
                name(r, true)
            )
        } else {
            format!(
                "{} pawn becomes a {}.",
                if yours { "Your" } else { "My" },
                name(r, false)
            )
        }
    } else if let Some((capture_square, r)) = capture(m) {
        let recapture = g.nodes[parent].parent.and_then(|prior| {
            let p=position(&g.nodes[prior].fen,g.chess960).ok()?;
            let previous=g.nodes[parent].uci.parse::<UciMove>().ok()?.to_move(&p).ok()?;
            capture(previous).map(|(sq,_)|sq==capture_square)
        }).unwrap_or(false);
        if recapture {
            match (yours,de) {
                (true,true)=>"Du schlägst zurück und gewinnst das Material zurück.",
                (false,true)=>"Ich schlage zurück und gewinne das Material zurück.",
                (true,false)=>"You're taking back the piece that just captured yours.",
                (false,false)=>"I'm taking back the piece that just captured mine.",
            }.into()
        } else {
        if de {
            if yours {
                format!("Du schlägst {}.", object(r, true, "mein"))
            } else {
                format!("Ich schlage {}.", object(r, true, "dein"))
            }
        } else if yours {
            format!("You've captured {}.", object(r, false, "my"))
        } else {
            format!("I'm taking {}.", object(r, false, "your"))
        }
        }
    } else if m.is_castle() {
        match (yours, de) {
            (true, true) => "Du hast rochiert und deinen Turm ins Spiel gebracht.",
            (false, true) => "Ich rochiere und bringe meinen Turm ins Spiel.",
            (true, false) => "You've castled and brought your rook into play.",
            (false, false) => "I'm castling and bringing my rook into play.",
        }
        .into()
    } else if let Move::Normal {
        role: r, from, to, ..
    } = m
    {
        let endangered = before
            .clone()
            .swap_turn()
            .ok()
            .is_some_and(|p| legal_capture(&p, from));
        if endangered && !legal_capture(&after, to) {
            if de {
                if yours {
                    format!(
                        "Du hast {} aus dem direkten Angriff befreit.",
                        object(r, true, "dein")
                    )
                } else {
                    format!(
                        "Ich bringe {} aus dem direkten Angriff.",
                        object(r, true, "mein")
                    )
                }
            } else if yours {
                format!(
                    "You've moved your {} out of the immediate attack.",
                    name(r, false)
                )
            } else {
                format!(
                    "I'm moving my {} out of the immediate attack.",
                    name(r, false)
                )
            }
        } else if let Some((_, target, ray)) = crate::coach_answers::verified_ray(g, node, de)? {
            let sq = target.parse::<Square>().map_err(|e| e.to_string())?;
            let front = after.board().role_at(sq).ok_or("Missing ray blocker")?;
            let absolute = ray.contains("pins ") || ray.contains("fesselt ");
            if de {
                if absolute {
                    format!(
                        "{} {} fesselt {} an den König.",
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
                        name(r, true),
                        object(front, true, if yours { "mein" } else { "dein" })
                    )
                } else {
                    "Hinter dem angegriffenen Bauern steht noch eine Figur. Ein Bauernzug könnte sie dem Angriff aussetzen.".into()
                }
            } else if absolute {
                format!(
                    "{} {} pins {} to the king.",
                    if yours { "Your" } else { "My" },
                    name(r, false),
                    object(front, false, if yours { "my" } else { "your" })
                )
            } else {
                "There's another piece behind the pawn under attack. Moving the pawn could expose that piece.".into()
            }
        } else if after.is_check() {
            match (yours, de) {
                (true, true) => {
                    "Du gibst Schach. Ich muss auf den Angriff auf meinen König reagieren."
                }
                (false, true) => "Schach! Du musst den Angriff auf deinen König beantworten.",
                (true, false) => "That's check. I have to answer the attack on my king.",
                (false, false) => "Check! You'll need to answer the attack on your king.",
            }
            .into()
        } else if let Some(target) = attack_target(&after, to) {
            if de {
                format!(
                    "{} {} greift {} an. Prüfe die Antwort und mögliche Rückschläge.",
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
                    name(r, true),
                    object(target, true, if yours { "mein" } else { "dein" })
                )
            } else {
                format!(
                    "{} {} attacks {}. Check the response and possible recaptures.",
                    if yours { "Your" } else { "My" },
                    name(r, false),
                    object(target, false, if yours { "my" } else { "your" })
                )
            }
        } else if r != Role::Pawn && new_pawn_defense(&before, &after, from, to) {
            match (yours,de) {
                (true,true)=>"Dein zuvor ungedeckter Bauer ist jetzt geschützt.",
                (false,true)=>"Ich habe meinen zuvor ungedeckten Bauern geschützt.",
                (true,false)=>"Your pawn was undefended. Now you've protected it.",
                (false,false)=>"I've protected my pawn. It was undefended before.",
            }.into()
        } else if r == Role::Pawn && bishop_mobility(&after,before.turn()) > bishop_mobility(&before,before.turn()) {
            let central=[Square::D4,Square::E4,Square::D5,Square::E5].contains(&to);
            if central {match (yours,de) {
                (true,true)=>"Dein Bauer besetzt das Zentrum und öffnet die Diagonale für deinen Läufer.",
                (false,true)=>"Mein Bauer gewinnt Raum im Zentrum und öffnet die Diagonale für meinen Läufer.",
                (true,false)=>"Your pawn takes space in the centre and opens a diagonal for your bishop.",
                (false,false)=>"My pawn takes space in the centre and opens a diagonal for my bishop.",
            }.into()} else { match (yours,de) {
                (true,true)=>"Du öffnest die Diagonale für deinen Läufer und gibst ihm mehr Bewegungsfreiheit.",
                (false,true)=>"Ich öffne die Diagonale für meinen Läufer, damit er ins Spiel kommen kann.",
                (true,false)=>"You're opening a diagonal for your bishop and giving it more room to move.",
                (false,false)=>"I'm opening a diagonal for my bishop so it can come into the game.",
            }.into() }
        } else if r == Role::Pawn && [Square::D4, Square::E4, Square::D5, Square::E5].contains(&to)
        {
            match(yours,de){(true,true)=>"Du besetzt das Zentrum mit deinem Bauern. Behalte die gegnerischen Bauernhebel im Blick.",(false,true)=>"Mein Bauer nimmt Raum im Zentrum ein. Schau, wie du dagegenhalten kannst.",(true,false)=>"Your pawn takes space in the centre. Keep an eye on the opposing pawn breaks.",(false,false)=>"My pawn takes space in the centre. Look for a way to challenge it."}.into()
        } else if [Role::Knight, Role::Bishop, Role::Queen].contains(&r)
            && from.rank() == before.turn().backrank()
        {
            let base = if de {
                if yours {
                    format!("Du bringst {} ins Spiel.", object(r, true, "dein"))
                } else {
                    format!("Ich bringe {} ins Spiel.", object(r, true, "mein"))
                }
            } else if yours {
                format!("You're bringing your {} into the game.", name(r, false))
            } else {
                format!("I'm bringing my {} into the game.", name(r, false))
            };
            if centre_control(&after, to) {
                format!(
                    "{base} {}",
                    if de {
                        "Von dort greift die Figur ins Zentrum."
                    } else {
                        "From there, the piece controls central squares."
                    }
                )
            } else {
                base
            }
        } else if centre_control(&after, to) && !centre_control(&before, from) {
            if de {
                format!("{} {} greift jetzt ins Zentrum. Achte darauf, dass die Figur auch sicher steht.",if yours{if r==Role::Queen{"Deine"}else{"Dein"}}else if r==Role::Queen{"Meine"}else{"Mein"},name(r,true))
            } else {
                format!(
                    "{} {} now controls central squares. Keep its safety in mind too.",
                    if yours { "Your" } else { "My" },
                    name(r, false)
                )
            }
        } else if r == Role::Pawn {
            let flank = if to.file() < shakmaty::File::D {
                if de {
                    "am Damenflügel"
                } else {
                    "on the queenside"
                }
            } else if to.file() > shakmaty::File::E {
                if de {
                    "am Königsflügel"
                } else {
                    "on the kingside"
                }
            } else {
                if de {
                    "im Zentrum"
                } else {
                    "in the centre"
                }
            };
            if de {
                format!(
                    "{} Bauer gewinnt Raum {flank}.",
                    if yours { "Dein" } else { "Mein" }
                )
            } else {
                format!(
                    "{} pawn takes space {flank}.",
                    if yours { "Your" } else { "My" }
                )
            }
        } else {
            let before_count = before
                .legal_moves()
                .iter()
                .filter(|m| m.from() == Some(from))
                .count();
            let after_count = after
                .clone()
                .swap_turn()
                .ok()
                .map(|p| {
                    p.legal_moves()
                        .iter()
                        .filter(|m| m.from() == Some(to))
                        .count()
                })
                .unwrap_or(0);
            if after_count > before_count {
                if de {
                    format!(
                        "{} {} hat jetzt mehr Bewegungsfreiheit.",
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
                        name(r, true)
                    )
                } else {
                    format!(
                        "{} {} now has more room to move.",
                        if yours { "Your" } else { "My" },
                        name(r, false)
                    )
                }
            } else if r == Role::Rook
                && !after
                    .board()
                    .pawns()
                    .into_iter()
                    .any(|sq| sq.file() == to.file())
            {
                if de {
                    format!(
                        "{} Turm steht jetzt auf einer offenen Linie.",
                        if yours { "Dein" } else { "Mein" }
                    )
                } else {
                    format!(
                        "{} rook is now on an open file.",
                        if yours { "Your" } else { "My" }
                    )
                }
            } else if de {
                format!(
                    "{} {} ist neu aufgestellt. Behalte die gegnerischen Angriffe im Blick.",
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
                    name(r, true)
                )
            } else {
                format!(
                    "{} {} is repositioned. Keep an eye on the opposing attacks.",
                    if yours { "Your" } else { "My" },
                    name(r, false)
                )
            }
        }
    } else {
        if de {
            "Schauen wir auf die Antwort."
        } else {
            "Let's look at the reply."
        }
        .into()
    };
    Ok(format!("{prefix}{text}"))
}

// This short explanation claims force only when every opposing reply in the
// replayed line is the sole legal move. A check or mate score alone is not proof.
fn forced_queen_knight_mate(before:&shakmaty::Chess,after:&shakmaty::Chess,m:&Move,s:&Search)->bool {
    if m.role()!=Role::Queen {return false;}
    let Some(line)=s.lines.first().filter(|line|!line.bound) else {return false;};
    let mut p=after.clone();
    for (index,text) in line.pv.iter().take(8).enumerate() {
        let Some(next)=text.parse::<UciMove>().ok().and_then(|u|u.to_move(&p).ok()) else {return false;};
        if index==0 && !matches!(next,Move::Normal{to,capture:Some(Role::Queen),..} if to==m.to()) {return false;}
        if p.turn()!=before.turn() && p.legal_moves().len()!=1 {return false;}
        let knight=next.role()==Role::Knight;
        p.play_unchecked(next);
        if p.is_checkmate() {return p.turn()!=before.turn() && knight;}
    }
    false
}

#[cfg(test)]
mod sacrifice_message_tests {
    use super::*;
    #[test]
    fn forced_queen_sacrifice_explains_knight_mate_in_both_languages_and_ownerships() {
        let mut g=Game::new("q4r1k/6pp/4Q2N/8/8/8/8/6K1 w - - 0 1".into(),"w".into(),400,0.,0.,false).unwrap();
        g.commit("e6g8",0,false).unwrap();
        let s=Search{best:"f8g8".into(),lines:vec![crate::engine::Line{depth:20,pv:vec!["f8g8".into(),"h6f7".into()],..Default::default()}],elapsed_ms:0.,engine:"Stockfish 19".into()};
        for (human,en,de) in [("w","your knight","deinem Springer"),("b","my knight","mein Springer")] {
            g.human=human.into();
            let english=message_with_alternative(&g,1,Some("Brilliant"),&s,"en",None).unwrap();
            let german=message_with_alternative(&g,1,Some("Brilliant"),&s,"de",None).unwrap();
            assert!(english.contains(en)&&english.contains("checkmate"));
            assert!(german.contains(de)&&german.contains("matt"));
        }
    }
    #[test]
    fn incomplete_or_illegal_line_does_not_claim_forced_sacrifice() {
        let mut g=Game::new("q4r1k/6pp/4Q2N/8/8/8/8/6K1 w - - 0 1".into(),"w".into(),400,0.,0.,false).unwrap();
        g.commit("e6g8",0,false).unwrap();
        for pv in [vec!["f8g8".into()],vec!["f8g8".into(),"h6h8".into()]] {
            let s=Search{best:"f8g8".into(),lines:vec![crate::engine::Line{depth:20,pv,..Default::default()}],elapsed_ms:0.,engine:"Stockfish 19".into()};
            let text=message_with_alternative(&g,1,Some("Brilliant"),&s,"en",None).unwrap();
            assert!(!text.contains("forced capture"));
        }
    }
}

pub fn answer(g: &Game, node: usize, q: &str, lang: &str) -> Result<String, String> {
    let de = lang == "de";
    let id = crate::coach_answers::answer_node(g, node, q);
    if q.contains("Why") && id == 0 {
        return Ok(if de {
            "Es wurde noch kein Zug gespielt. Schauen wir auf deine Möglichkeiten."
        } else {
            "No move has been played yet. Let's look at your options."
        }
        .into());
    }
    if q.contains("Why") || q == "Review this move" {
        let e = g.nodes[id]
            .evidence
            .as_ref()
            .ok_or("Move evidence pending")?;
        let search =
            serde_json::from_value::<Search>(e["search"].clone()).map_err(|e| e.to_string())?;
        return message(g, id, e["label"].as_str(), &search, lang);
    }
    let p = position(&g.nodes[node].fen, g.chess960)?;
    if g.human != "both" && (p.turn() == Color::White) != (g.human == "w") {
        return Ok(if de {
            "Warten wir auf meinen Zug. Danach schauen wir auf deine Möglichkeiten."
        } else {
            "Let's wait for my move. Then we'll look at your options."
        }
        .into());
    }
    let e = g.nodes[node]
        .evidence
        .as_ref()
        .ok_or("Position analysis is still running")?;
    let s = serde_json::from_value::<Search>(e["search"].clone()).map_err(|e| e.to_string())?;
    let Some(line) = s.lines.first() else {
        return Ok(if de {
            "Hier gibt es keine legalen Züge mehr."
        } else {
            "There are no legal moves left here."
        }
        .into());
    };
    let mut p = p;
    for (i, u) in line.pv.iter().take(2).enumerate() {
        let m = u
            .parse::<UciMove>()
            .map_err(|e| e.to_string())?
            .to_move(&p)
            .map_err(|e| e.to_string())?;
        if q.contains("threat") && i == 1 {
            if let Some((_, r)) = capture(m) {
                return Ok(if de {
                    format!("Pass auf. Meine Antwort könnte {} angreifen. Prüfe die Antwort, bevor du ziehst.",object(r,true,"dein"))
                } else {
                    format!(
                        "Watch out. My reply could attack {}. Check the response before you move.",
                        object(r, false, "your")
                    )
                });
            }
            p.play_unchecked(m);
            if p.is_check() {
                return Ok(if de {
                    "Nach deiner Antwort könnte ich Schach geben. Behalte deinen König im Blick."
                } else {
                    "After your move, I could give check. Keep an eye on your king."
                }
                .into());
            }
            continue;
        }
        if !q.contains("threat") && i == 0 {
            return Ok(if let Some((_, r)) = capture(m) {
                if de {
                    format!(
                        "Schau, ob du {} schlagen kannst. Prüfe auch den Rückschlag.",
                        object(r, true, "mein")
                    )
                } else {
                    format!(
                        "Look for a chance to capture {}. Check the recapture too.",
                        object(r, false, "my")
                    )
                }
            } else if m.is_castle() {
                if de {
                    "Überlege, ob du jetzt rochieren möchtest."
                } else {
                    "Consider castling now."
                }
                .into()
            } else {
                let r = m.role();
                if de {
                    format!(
                        "Schau dir {} an. Überlege, wie du diese Figur besser ins Spiel bringst.",
                        object(r, true, "dein")
                    )
                } else {
                    format!(
                        "Take a look at your {}. Think about how to bring it into the game.",
                        name(r, false)
                    )
                }
            });
        }
        p.play_unchecked(m)
    }
    Ok(if de{"Ich sehe hier noch keine konkrete taktische Drohung. Schau trotzdem auf deine ungeschützten Figuren."}else{"I haven't found a concrete tactical threat here yet. Keep an eye on your undefended pieces."}.into())
}

/// Project historical evidence to the current conversational style without
/// rewriting the user's stored games or waiting for a new engine search.
#[tauri::command]
pub fn coach_messages(
    a: tauri::State<crate::App>,
    game_id: String,
    revision: u64,
    nodes: Vec<usize>,
) -> Result<std::collections::HashMap<String, String>, String> {
    if nodes.len() > 2 {
        return Err("At most two visible messages".into());
    }
    let g = a.game.lock().unwrap().clone();
    if g.id != game_id || g.revision != revision {
        return Err("Stale coach message request".into());
    }
    let lang = crate::coach_answers::language(&a);
    let mut out = std::collections::HashMap::new();
    for node in nodes {
        let n = g.nodes.get(node).ok_or("Unknown coach position")?;
        if let Some(e) = &n.evidence {
            let s =
                serde_json::from_value::<Search>(e["search"].clone()).map_err(|e| e.to_string())?;
            out.insert(
                format!("{game_id}:{node}"),
                message(&g, node, e["label"].as_str(), &s, &lang)?,
            );
        }
    }
    Ok(out)
}

/// Immediate verified answer; model wording is an optional later refinement.
#[tauri::command]
pub fn coach_reply(
    a: tauri::State<crate::App>,
    game_id: String,
    revision: u64,
    node: usize,
    question: String,
) -> Result<String, String> {
    let g = a.game.lock().unwrap().clone();
    if g.id != game_id || g.revision != revision {
        return Err("Stale coach answer request".into());
    }
    if node >= g.nodes.len() {
        return Err("Unknown coach position".into());
    }
    answer(&g, node, &question, &crate::coach_answers::language(&a))
}

/// The AI may choose tone, but every option preserves the complete checked claim.
pub fn wording_choices(text: &str) -> Vec<String> {
    let mut options = vec![text.to_owned()];
    for (a, b, c) in [
        ("Careful! ", "Watch out. ", "Take a closer look. "),
        ("Pass auf! ", "Vorsicht. ", "Schau noch einmal hin. "),
        ("Great move! ", "Well spotted! ", "That's a strong move. "),
        ("Stark gespielt! ", "Gut gesehen! ", "Das ist stark. "),
        ("Nicely done! ", "Well played! ", "Nice work. "),
        (
            "Sehr schön! ",
            "Gut gespielt! ",
            "Das hast du gut gemacht. ",
        ),
    ] {
        if let Some(rest) = text.strip_prefix(a) {
            options.push(format!("{b}{rest}"));
            options.push(format!("{c}{rest}"));
            break;
        }
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;
    fn paired_game(moves:&str)->Game {
        let mut g=Game::new(crate::game::START.into(),"both".into(),100,0.,0.,false).unwrap();
        for m in moves.split_whitespace(){g.commit(m,g.revision,false).unwrap();}
        g
    }
    #[test]
    fn paired_commentary_uses_actor_and_concrete_visible_purpose() {
        for (moves,reply,en,de) in [
            ("e2e4 d7d5 e4d5 d8d5 b1c3","d5d8","attacks my queen","greift meine Dame"),
            ("e2e4 d7d5 e4d5 d8d5 b1c3 d5d8 h2h4 g7g6","d2d4","my bishop","meinen Läufer"),
            ("e2e4 d7d5 e4d5 d8d5 b1c3 d5d8 h2h4 g7g6 d2d4 f8h6 c1h6 g8h6","g1f3","I'm taking back","Ich schlage zurück"),
            ("e2e4 d7d5 e4d5 d8d5 b1c3 d5d8 h2h4 g7g6 d2d4 f8h6 c1h6 g8h6 d1h5 g6h5 g1f3","h6g8","protected it","jetzt geschützt"),
            ("e2e4 d7d5 e4d5 d8d5 b1c3 d5d8 h2h4 g7g6 d2d4 f8h6 c1h6 g8h6 d1h5 g6h5 g1f3 h6g8 e1c1 b8c6 f1b5","e8d7","pins my knight","fesselt meinen Springer"),
        ] {
            let g=paired_game(moves);let original=serde_json::to_string(&g).unwrap();
            for (lang,needle) in [("en",en),("de",de)] {
                let text=message(&g,g.live,Some("Excellent"),&search(&[reply]),lang).unwrap();
                assert!(text.contains(needle),"{moves}: {text}");no_notation(&text);
            }
            assert_eq!(serde_json::to_string(&g).unwrap(),original);
        }
    }
    #[test]
    fn defense_excludes_pinned_piece_and_includes_pawn_capture() {
        let pinned=position("4r2k/8/8/8/3P4/8/4N3/4K3 w - - 0 1",false).unwrap();
        assert!(!legal_defenders(&pinned,Square::D4,Color::White).contains(&Square::E2));
        let pawn=position("7k/8/8/8/3P4/2P5/8/K7 w - - 0 1",false).unwrap();
        assert!(legal_defenders(&pawn,Square::D4,Color::White).contains(&Square::C3));
    }
    #[test]
    fn pawn_pressure_requires_an_actual_absolute_pin() {
        let g=paired_game("e2e4 d7d5 e4d5 d8d5 b1c3 d5d8 h2h4 g7g6 d2d4 f8h6 c1h6 g8h6 d1h5 g6h5 g1f3 h6g8 e1c1 b8c6 f1b5 e8d7");
        let text=message(&g,g.live,Some("Blunder"),&search(&["d4d5"]),"en").unwrap();
        assert!(text.contains("My knight is pinned")&&text.contains("pawn attack"),"{text}");
        no_notation(&text);
        let unpinned=position("k2q2nr/ppp1pp1p/2n5/1B5p/3P3P/2N2N2/PPP2PP1/2KR3R w - - 0 11",false).unwrap();
        assert!(pawn_pressure_on_pin(&unpinned,&search(&["d4d5"])).is_none());
    }
    #[test]
    fn distant_unrelated_trade_never_obscures_a_hanging_queen() {
        let g=paired_game("e2e4 d7d5 e4d5 d8d5 b1c3 d5d8 h2h4 g7g6 d2d4 f8h6 c1h6 g8h6 d1h5");
        let s=search(&["g6h5","f1e2","c7c6","g2g4","c8d7","g4g5","h6g4","e1c1","a7a5","e2g4","h5g4"]);
        for (lang,needle) in [("en","your queen"),("de","deine Dame")] {
            let t=message(&g,g.live,Some("Blunder"),&s,lang).unwrap();
            assert!(t.contains(needle),"{t}");no_notation(&t);
        }
    }
    #[test]
    fn poor_exchange_names_the_actual_knight_relocation_without_free_material() {
        let g=paired_game("e2e4 d7d5 e4d5 d8d5 b1c3 d5d8 h2h4 g7g6 d2d4 f8h6");
        let t=message(&g,g.live,Some("Mistake"),&search(&["c1h6","g8h6"]),"en").unwrap();
        assert!(t.contains("my knight")&&t.contains("edge of the board"),"{t}");
        assert!(!t.contains("win material"));no_notation(&t);
    }
    fn search(pv: &[&str]) -> Search {
        Search {
            lines: vec![crate::engine::Line {
                pv: pv.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            }],
            best: pv.first().unwrap_or(&"").to_string(),
            elapsed_ms: 0.,
            engine: "legal fixture".into(),
        }
    }
    fn no_notation(text: &str) {
        assert!(
            !text
                .as_bytes()
                .windows(2)
                .any(|w| (b'a'..=b'h').contains(&w[0]) && (b'1'..=b'8').contains(&w[1])),
            "{text}"
        );
        assert!(text.chars().count() < 230, "{text}")
    }
    #[test]
    fn ordinary_feedback_is_bilingual_and_not_a_pv_dump() {
        let mut g = Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
        for u in ["e2e4", "e7e5", "g1f3"] {
            g.commit(u, g.revision, false).unwrap();
            for lang in ["en", "de"] {
                let t = message(&g, g.live, Some("Book"), &search(&[]), lang).unwrap();
                no_notation(&t);
                assert!(!t.contains("Fortsetzung"));
            }
        }
        assert!(message(&g, g.live, Some("Excellent"), &search(&[]), "de")
            .unwrap()
            .contains("greift meinen Bauern an"));
    }
    #[test]
    fn blunder_keeps_real_piece_consequence_without_coordinates() {
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
        for (human, victim) in [("w", "your knight"), ("b", "my knight")] {
            g.human = human.into();
            let t = message(&g, 1, Some("Blunder"), &search(&["d4d5"]), "en").unwrap();
            assert!(t.contains(victim));
            assert!(t.contains("win material"));
            no_notation(&t);
            let de = message(&g, 1, Some("Blunder"), &search(&["d4d5"]), "de").unwrap();
            assert!(de.contains("Springer"));
            no_notation(&de);
        }
    }
    #[test]
    fn exchange_then_extra_pawn_is_not_a_lost_knight_claim() {
        let mut g = Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
        for u in [
            "e2e4", "c7c6", "g1f3", "d8c7", "b1c3", "h7h6", "f1c4", "g8f6", "e4e5", "f6d5",
        ] {
            g.commit(u, g.revision, false).unwrap();
        }
        let s = search(&["c4d5", "c6d5", "c3d5"]);
        let t = message(&g, g.live, Some("Blunder"), &s, "en").unwrap();
        assert!(t.contains("trade") && t.contains("pawn"));
        assert!(!t.contains("knight"));
        no_notation(&t);
    }
    #[test]
    fn equal_recapture_does_not_claim_a_material_win() {
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
        let t = message(&g, 1, Some("Mistake"), &search(&["e5d4", "c3d4"]), "en").unwrap();
        assert!(!t.contains("win material"));
        no_notation(&t);
    }
    #[test]
    fn bishop_rescue_is_verified_against_legal_captures() {
        let mut g = Game::new(
            "7k/8/8/4p3/3B4/8/8/K7 w - - 0 1".into(),
            "w".into(),
            400,
            0.,
            0.,
            false,
        )
        .unwrap();
        g.commit("d4e3", 0, false).unwrap();
        let t = message(&g, 1, Some("Good"), &search(&[]), "en").unwrap();
        assert!(t.contains("bishop out of the immediate attack"));
        no_notation(&t);
    }
    #[test]
    fn castles_and_underpromotions_use_actual_move_and_german_grammar() {
        let mut g = Game::new(
            "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1".into(),
            "w".into(),
            400,
            0.,
            0.,
            false,
        )
        .unwrap();
        g.commit("e1g1", 0, false).unwrap();
        assert!(message(&g, 1, None, &search(&[]), "en")
            .unwrap()
            .contains("castled"));
        let mut g = Game::new(
            "7k/P7/8/8/8/8/8/K7 w - - 0 1".into(),
            "w".into(),
            400,
            0.,
            0.,
            false,
        )
        .unwrap();
        g.commit("a7a8n", 0, false).unwrap();
        let t = message(&g, 1, None, &search(&[]), "de").unwrap();
        assert!(t.contains("zu einem Springer"));
        no_notation(&t);
    }
    #[test]
    fn illegal_evidence_is_rejected_and_mate_is_qualified() {
        let mut g = Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
        for u in ["f2f3", "e7e5", "g2g4"] {
            g.commit(u, g.revision, false).unwrap();
        }
        assert!(message(&g, 3, Some("Blunder"), &search(&["a1a8"]), "en").is_err());
        let t = message(&g, 3, Some("Blunder"), &search(&["d8h4"]), "en").unwrap();
        assert!(t.contains("path to checkmate"));
        no_notation(&t);
    }
    #[test]
    fn ai_options_retain_complete_causal_sentence() {
        let text = "Careful! Your opponent can capture your queen and win material.";
        let options = wording_choices(text);
        assert_eq!(options.len(), 3);
        for option in options {
            assert!(option.ends_with("Your opponent can capture your queen and win material."));
            no_notation(&option);
        }
    }
    #[test]
    fn why_reprojects_historical_evidence_without_mutating_it() {
        let mut g = Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
        g.commit("e2e4", 0, false).unwrap();
        g.nodes[1].evidence = Some(
            serde_json::json!({"label":"Book","explanation":"e4 e5 Nf3 Nc6 old technical copy","search":search(&["e7e5"])}),
        );
        let old = serde_json::to_string(&g).unwrap();
        let t = answer(&g, 1, "Why was that a mistake?", "en").unwrap();
        no_notation(&t);
        assert!(t.contains("centre"));
        assert_eq!(serde_json::to_string(&g).unwrap(), old);
    }
    #[test]
    fn delayed_recapture_is_not_a_free_piece() {
        let mut g = Game::new(
            "7k/8/4r3/8/3P4/2P5/8/K7 w - - 0 1".into(),
            "w".into(),
            400,
            0.,
            0.,
            false,
        )
        .unwrap();
        g.commit("a1b1", 0, false).unwrap();
        let s = search(&[
            "e6d6", "b1a1", "d6d4", "a1b1", "h8g8", "b1a1", "d4b4", "c3b4",
        ]);
        let t = message(&g, 1, Some("Mistake"), &s, "en").unwrap();
        assert!(!t.contains("win material"), "{t}");
        no_notation(&t);
    }
    #[test]
    fn promotion_gain_is_explained_as_promotion() {
        let mut g = Game::new(
            "7k/8/8/8/8/8/6p1/K7 w - - 0 1".into(),
            "w".into(),
            400,
            0.,
            0.,
            false,
        )
        .unwrap();
        g.commit("a1b1", 0, false).unwrap();
        for lang in ["en", "de"] {
            let t = message(&g, 1, Some("Blunder"), &search(&["g2g1q"]), lang).unwrap();
            assert!(t.contains(if lang == "de" { "umwandeln" } else { "promote" }));
            no_notation(&t);
        }
    }
    #[test]
    fn pinned_piece_is_not_a_legal_capture_threat() {
        let mut g = Game::new(
            "4k3/4n3/8/3Q4/8/8/8/K3R3 b - - 0 1".into(),
            "w".into(),
            400,
            0.,
            0.,
            false,
        )
        .unwrap();
        g.commit("e8f8", 0, false).unwrap();
        let pinned = position("4k3/4n3/8/3Q4/8/8/8/K3R3 w - - 0 1", false).unwrap();
        assert_eq!(attack_target(&pinned, Square::E7), None);
        let unpinned = position(&g.nodes[1].fen, false).unwrap();
        assert_eq!(attack_target(&unpinned, Square::E7), Some(Role::Queen));
    }
    #[test]
    fn quiet_error_explains_legal_alternative_in_both_languages_and_ownerships() {
        for (human, english, german) in [
            ("w", "your knight", "deinen Springer"),
            ("b", "my knight", "meinen Springer"),
        ] {
            let mut g =
                Game::new(crate::game::START.into(), human.into(), 400, 0., 0., false).unwrap();
            g.commit("a2a3", 0, false).unwrap();
            for alt in ["Nf3", "g1f3"] {
                let en = crate::coach_answers::friendly(
                    &g,
                    1,
                    Some("Mistake"),
                    Some(alt),
                    &search(&["e7e5"]),
                    "en",
                )
                .unwrap();
                assert!(
                    en.contains(english) && en.contains("central squares"),
                    "{en}"
                );
                let de = crate::coach_answers::friendly(
                    &g,
                    1,
                    Some("Mistake"),
                    Some(alt),
                    &search(&["e7e5"]),
                    "de",
                )
                .unwrap();
                assert!(de.contains(german) && de.contains("Zentrum"), "{de}");
                assert!(de.ends_with(" sollen.") && !de.contains("gewesen,"), "{de}");
                no_notation(&en);
                no_notation(&de);
                assert!(!en.contains("isn't established") && !en.contains("Explain"));
            }
        }
    }
    #[test]
    fn historical_quiet_feedback_uses_stored_alternative_without_mutating_game() {
        let mut g = Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
        g.commit("a2a3", 0, false).unwrap();
        g.nodes[1].evidence = Some(serde_json::json!({"best_alternative":"Nf3"}));
        let original = serde_json::to_string(&g).unwrap();
        let text = message(&g, 1, Some("Inaccuracy"), &search(&["e7e5"]), "en").unwrap();
        assert!(
            text.contains("knight") && text.contains("central squares"),
            "{text}"
        );
        assert_eq!(serde_json::to_string(&g).unwrap(), original);
    }
    #[test]
    fn quiet_capture_alternative_names_opportunity_without_inventing_free_material() {
        let mut g = Game::new(
            "7k/8/8/4p3/3P4/8/8/K7 w - - 0 1".into(),
            "w".into(),
            400,
            0.,
            0.,
            false,
        )
        .unwrap();
        g.commit("a1b1", 0, false).unwrap();
        let text = message_with_alternative(
            &g,
            1,
            Some("Mistake"),
            &search(&["h8g8"]),
            "en",
            Some("d4e5"),
        )
        .unwrap();
        assert!(text.contains("capture my pawn"), "{text}");
        assert!(!text.contains("win material"));
        no_notation(&text);
    }
    #[test]
    fn illegal_alternative_never_becomes_a_coach_recommendation() {
        let mut g = Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
        g.commit("a2a3", 0, false).unwrap();
        let text = message_with_alternative(
            &g,
            1,
            Some("Mistake"),
            &search(&["g8f6"]),
            "en",
            Some("f1b5"),
        )
        .unwrap();
        assert!(
            !text.contains("bishop") && !text.contains("stronger option"),
            "{text}"
        );
        assert!(
            text.contains("opponent")
                && text.contains("knight")
                && text.contains("central squares")
        );
        no_notation(&text);
    }
    #[test]
    fn quiet_positive_feedback_explains_pawn_space_without_uncertainty_boilerplate() {
        for (uci, flank) in [("a2a3", "queenside"), ("h2h3", "kingside")] {
            let mut g =
                Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
            g.commit(uci, 0, false).unwrap();
            for lang in ["en", "de"] {
                let text = message(&g, 1, Some("Good"), &search(&["e7e5"]), lang).unwrap();
                assert!(
                    !text.contains("established") && !text.contains("belegt"),
                    "{text}"
                );
                assert!(
                    text.contains(if lang == "en" {
                        flank
                    } else if flank == "queenside" {
                        "Damenflügel"
                    } else {
                        "Königsflügel"
                    }),
                    "{text}"
                );
                no_notation(&text);
            }
        }
    }
}
