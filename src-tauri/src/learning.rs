//! Local learning storage. Original games are never modified by these commands.
use crate::{
    game::{position, Game, Snapshot},
    App,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use shakmaty::{uci::UciMove, Color, Position};
use std::{
    collections::{BTreeMap, HashSet},
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Serialize, Deserialize)]
struct Puzzle {
    id: String,
    fen: String,
    moves: Vec<String>,
    rating: i32,
    themes: Vec<String>,
    url: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Card {
    id: String,
    fen: String,
    solution: Vec<String>,
    rating: i32,
    themes: Vec<String>,
    title: String,
    explanation: String,
    source: String,
    game_id: Option<String>,
    node: Option<usize>,
    chess960: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Session {
    id: String,
    card: Card,
    moves: Vec<String>,
    assisted: bool,
    failed: bool,
    complete: bool,
    recorded: bool,
}
#[derive(Default, Clone, Serialize, Deserialize)]
struct Progress {
    wins: u32,
    attempts: u32,
    streak: u32,
    interval: u64,
    due: u64,
    last: u64,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn schema(db: &Connection) -> Result<(), String> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS learning_progress(id TEXT PRIMARY KEY,data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS learning_sessions(id TEXT PRIMARY KEY,data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS study_variations(id TEXT PRIMARY KEY,game_id TEXT NOT NULL,origin INTEGER NOT NULL,fen TEXT NOT NULL,moves TEXT NOT NULL,note TEXT NOT NULL,updated INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS study_game ON study_variations(game_id);").map_err(|e|e.to_string())
}
fn play(fen: &str, moves: &[String], c960: bool, id: &str) -> Result<Game, String> {
    let mut g = Game::new(fen.into(), "both".into(), 400, 0., 0., c960)?;
    g.id = id.into();
    for u in moves {
        g.result = None;
        let rev = g.revision;
        g.commit(u, rev, false)?;
    }
    Ok(g)
}
fn corpus() -> Result<&'static Vec<Puzzle>, String> {
    static DATA: OnceLock<Result<Vec<Puzzle>, String>> = OnceLock::new();
    DATA.get_or_init(|| {
        let puzzles: Vec<Puzzle> =
            serde_json::from_str(include_str!("../resources/learning/puzzles.json"))
                .map_err(|e| e.to_string())?;
        let mut ids = HashSet::new();
        for p in &puzzles {
            if !ids.insert(&p.id) || p.moves.len() < 2 {
                return Err("Duplicate or incomplete puzzle".into());
            }
            play(&p.fen, &p.moves, false, &p.id).map_err(|e| format!("Puzzle {}: {e}", p.id))?;
        }
        Ok(puzzles)
    })
    .as_ref()
    .map_err(Clone::clone)
}
fn puzzle_card(p: &Puzzle) -> Result<Card, String> {
    let setup = play(&p.fen, &p.moves[..1], false, &p.id)?;
    Ok(Card {
        id: format!("puzzle:{}", p.id),
        fen: setup.nodes[setup.live].fen.clone(),
        solution: p.moves[1..].to_vec(),
        rating: p.rating,
        themes: p.themes.clone(),
        title: "Tactical puzzle".into(),
        explanation: "Find the strongest continuation. Look for checks, captures and threats."
            .into(),
        source: p.url.clone(),
        game_id: None,
        node: None,
        chess960: false,
    })
}
fn mistakes(db: &Connection) -> Result<Vec<Card>, String> {
    let mut q = db
        .prepare("SELECT data FROM games")
        .map_err(|e| e.to_string())?;
    let rows = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut cards = vec![];
    for row in rows {
        let Ok(g) = serde_json::from_str::<Game>(&row.map_err(|e| e.to_string())?) else {
            continue;
        };
        for node in g.path(g.live).into_iter().skip(1) {
            let n = &g.nodes[node];
            let Some(parent) = n.parent else { continue };
            let Some(e) = &n.evidence else { continue };
            if !["Inaccuracy", "Mistake", "Blunder", "Miss"]
                .contains(&e["label"].as_str().unwrap_or(""))
            {
                continue;
            }
            let pos = position(&g.nodes[parent].fen, g.chess960)?;
            if g.human != "both" && (pos.turn() == Color::White) != (g.human == "w") {
                continue;
            }
            let Some(alt) = e["best_alternative"].as_str() else {
                continue;
            };
            if alt == n.uci {
                continue;
            }
            let Some(m) = alt
                .parse::<UciMove>()
                .ok()
                .and_then(|u| u.to_move(&pos).ok())
                .or_else(|| {
                    pos.legal_moves().into_iter().find(|m| {
                        shakmaty::san::SanPlus::from_move(pos.clone(), *m).to_string() == alt
                    })
                })
            else {
                continue;
            };
            let alt_uci = crate::game::uci(&pos, &m);
            if alt_uci == n.uci {
                continue;
            }
            let mut solution = vec![alt_uci.clone()];
            if let Some(pe) = &g.nodes[parent].evidence {
                if let Some(lines) = pe["search"]["lines"].as_array() {
                    if let Some(line) = lines.iter().find(|l| {
                        l["pv"][0].as_str() == Some(alt_uci.as_str())
                            && l["depth"].as_u64().unwrap_or(0) >= 8
                            && !l["bound"].as_bool().unwrap_or(true)
                    }) {
                        let pv: Vec<String> = line["pv"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter_map(|u| u.as_str().map(str::to_owned))
                            .take(5)
                            .collect();
                        let take = if pv.len() % 2 == 0 {
                            pv.len() - 1
                        } else {
                            pv.len()
                        };
                        if take > 0
                            && play(&g.nodes[parent].fen, &pv[..take], g.chess960, "validate")
                                .is_ok()
                        {
                            solution = pv[..take].to_vec();
                        }
                    }
                }
            }
            let mut themes = vec!["calculation".into()];
            let after = position(&n.fen, g.chess960)?;
            if let Some(reply) = e["search"]["lines"][0]["pv"][0].as_str() {
                if let Some(r) = reply
                    .parse::<UciMove>()
                    .ok()
                    .and_then(|u| u.to_move(&after).ok())
                {
                    if r.is_capture() {
                        themes.push("pieceSafety".into())
                    }
                    let mut end = after.clone();
                    end.play_unchecked(r);
                    if end.is_check() {
                        themes.push("kingSafety".into())
                    }
                }
            }
            let mut better = pos.clone();
            better.play_unchecked(m);
            if better.is_check() {
                themes.push("forcingMoves".into())
            }
            let detail = e["detail"].as_str().filter(|s| !s.is_empty()).unwrap_or(
                "This move lost ground. Find the better continuation from the position before it.",
            );
            cards.push(Card {
                id: format!("mistake:{}:{node}", g.id),
                fen: g.nodes[parent].fen.clone(),
                solution,
                rating: g.difficulty.max(600) as i32,
                themes,
                title: g.name.clone(),
                explanation: detail.into(),
                source: "Your saved game · verified Stockfish alternative".into(),
                game_id: Some(g.id.clone()),
                node: Some(parent),
                chess960: g.chess960,
            });
        }
    }
    Ok(cards)
}
fn progress(db: &Connection, id: &str) -> Progress {
    db.query_row(
        "SELECT data FROM learning_progress WHERE id=?1",
        [id],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|s| serde_json::from_str(&s).ok())
    .unwrap_or_default()
}
fn progress_map(db: &Connection) -> Result<BTreeMap<String, Progress>, String> {
    let mut q = db
        .prepare("SELECT id,data FROM learning_progress")
        .map_err(|e| e.to_string())?;
    let rows = q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut map = BTreeMap::new();
    for r in rows {
        let (id, data) = r.map_err(|e| e.to_string())?;
        map.insert(id, serde_json::from_str(&data).map_err(|e| e.to_string())?);
    }
    Ok(map)
}
fn rating(db: &Connection) -> i32 {
    db.query_row(
        "SELECT value FROM settings WHERE key='learning_rating'",
        [],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|s| s.parse().ok())
    .unwrap_or(1200)
}
fn schedule(mut p: Progress, clean: bool, time: u64) -> Progress {
    p.attempts += 1;
    p.last = time;
    if clean {
        p.wins += 1;
        p.streak += 1;
        p.interval = match p.streak {
            1 => 86400,
            2 => 3 * 86400,
            _ => (p.interval.max(86400) * 2).min(90 * 86400),
        };
    } else {
        p.streak = 0;
        p.interval = 600;
    }
    p.due = time + p.interval;
    p
}
fn record(db: &Connection, s: &mut Session) -> Result<(), String> {
    if s.recorded {
        return Ok(());
    }
    let clean = s.complete && !s.assisted && !s.failed;
    let p = schedule(progress(db, &s.card.id), clean, now());
    db.execute(
        "INSERT OR REPLACE INTO learning_progress VALUES(?1,?2)",
        params![s.card.id, serde_json::to_string(&p).unwrap()],
    )
    .map_err(|e| e.to_string())?;
    if s.card.game_id.is_none() {
        let old = rating(db);
        let expected = 1. / (1. + 10_f64.powf((s.card.rating - old) as f64 / 400.));
        let next = (old as f64 + 24. * ((if clean { 1. } else { 0. }) - expected))
            .round()
            .clamp(400., 2800.) as i32;
        db.execute(
            "INSERT OR REPLACE INTO settings VALUES('learning_rating',?1)",
            [next.to_string()],
        )
        .map_err(|e| e.to_string())?;
    }
    s.recorded = true;
    Ok(())
}
fn store_session(db: &Connection, s: &Session) -> Result<(), String> {
    db.execute(
        "INSERT OR REPLACE INTO learning_sessions VALUES(?1,?2)",
        params![s.id, serde_json::to_string(s).unwrap()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn response(s: &Session) -> Result<Value, String> {
    let mut g = play(&s.card.fen, &s.moves, s.card.chess960, &s.id)?;
    Ok(
        json!({"session":s.id,"snapshot":g.snapshot(None)?,"cursor":s.moves.len(),"complete":s.complete,"assisted":s.assisted,"failed":s.failed,"card":{"id":s.card.id,"title":s.card.title,"rating":s.card.rating,"themes":s.card.themes,"source":s.card.source,"game_id":s.card.game_id,"node":s.card.node},"explanation":if s.recorded{Some(&s.card.explanation)}else{None}}),
    )
}
fn overview(db: &Connection, with_corpus: bool) -> Result<Value, String> {
    let cards = mistakes(db)?;
    let mut patterns: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut due = 0;
    let mut practiced = 0;
    for c in &cards {
        let p = progress(db, &c.id);
        if p.due <= now() {
            due += 1
        }
        if p.wins > 0 {
            practiced += 1
        }
        for t in &c.themes {
            let row = patterns.entry(t.clone()).or_default();
            row.0 += 1;
            if p.streak > 0 {
                row.1 += 1
            }
        }
    }
    let puzzles: &[Puzzle] = if with_corpus {
        corpus()?.as_slice()
    } else {
        &[]
    };
    let mut q = db
        .prepare("SELECT id,data FROM learning_progress")
        .map_err(|e| e.to_string())?;
    let rows = q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut puzzle_due = 0;
    let mut attempts = 0;
    let mut wins = 0;
    for r in rows {
        let (id, data) = r.map_err(|e| e.to_string())?;
        let p: Progress = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        attempts += p.attempts;
        wins += p.wins;
        if id.starts_with("puzzle:") && p.due <= now() {
            puzzle_due += 1
        }
    }
    let mut themes: BTreeMap<String, usize> = BTreeMap::new();
    for p in puzzles {
        for t in &p.themes {
            *themes.entry(t.clone()).or_default() += 1
        }
    }
    let sample:Vec<_>=cards.iter().map(|c|json!({"id":c.id,"title":c.title,"themes":c.themes,"due":progress(db,&c.id).due,"node":c.node,"game_id":c.game_id})).collect();
    Ok(
        json!({"puzzles":puzzles.len(),"mistakes":cards.len(),"due_mistakes":due,"practiced_mistakes":practiced,"due_puzzles":puzzle_due,"attempts":attempts,"wins":wins,"rating":rating(db),"rating_note":"Local practice estimate; not a platform rating","patterns":patterns.iter().map(|(theme,(count,mastered))|json!({"theme":theme,"count":count,"mastered":mastered})).collect::<Vec<_>>(),"themes":themes,"cards":sample}),
    )
}

pub(crate) fn dispatch(a: &App, action: &str, args: &Value) -> Result<Value, String> {
    // Clone game before taking DB mutex: match the normal game → DB lock order.
    if action.starts_with("study_") {
        return studies(a, action, args);
    }
    if action == "threat" {
        return threat(a, args);
    }
    // Corpus validation never holds the profile/game storage mutex. The live
    // coaching nudge uses the light profile route and does not initialize it.
    if action == "overview" || (action == "start" && args["kind"].as_str() != Some("mistakes")) {
        corpus()?;
    }
    let connection = a.db.lock().unwrap();
    schema(&connection)?;
    let db = connection
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    let result = (|| match action {
        "overview" => overview(&db, true),
        "profile" => overview(&db, false),
        "start" => {
            let kind = args["kind"].as_str().unwrap_or("puzzles");
            let theme = args["theme"].as_str().unwrap_or("");
            let requested = args["id"].as_str();
            let game_id = args["game_id"].as_str();
            let previous = args["previous"].as_str().unwrap_or("");
            let mut card = if kind == "mistakes" {
                let mut cards = mistakes(&db)?;
                cards.retain(|c| {
                    game_id.is_none_or(|id| c.game_id.as_deref() == Some(id))
                        && (theme.is_empty() || c.themes.iter().any(|t| t == theme))
                        && requested.is_none_or(|id| c.id == id)
                });
                cards.sort_by_key(|c| {
                    let p = progress(&db, &c.id);
                    (c.id == previous, p.due, p.last)
                });
                cards
                    .into_iter()
                    .next()
                    .ok_or("No analyzed mistakes match this practice. Review a saved game first.")?
            } else {
                let target = args["rating"]
                    .as_i64()
                    .unwrap_or(rating(&db) as i64)
                    .clamp(400, 2800) as i32;
                let mut candidates: Vec<_> = corpus()?
                    .iter()
                    .filter(|p| {
                        (theme.is_empty() || p.themes.iter().any(|t| t == theme))
                            && requested.is_none_or(|id| format!("puzzle:{}", p.id) == id)
                    })
                    .collect();
                let history = progress_map(&db)?;
                candidates.sort_by_cached_key(|p| {
                    let id = format!("puzzle:{}", p.id);
                    let pr = history.get(&id).cloned().unwrap_or_default();
                    (
                        id == previous,
                        if pr.attempts > 0 && pr.due <= now() {
                            0
                        } else if pr.attempts == 0 {
                            1
                        } else {
                            2
                        },
                        (p.rating - target).abs(),
                        pr.last,
                        p.id.clone(),
                    )
                });
                puzzle_card(candidates.first().ok_or("No puzzle matches this theme")?)?
            };
            let de = args["language"].as_str() == Some("de");
            card.explanation = if let Some(id) = &card.game_id {
                let data: String = db
                    .query_row("SELECT data FROM games WHERE id=?1", [id], |r| r.get(0))
                    .map_err(|e| e.to_string())?;
                let source: Game = serde_json::from_str(&data).map_err(|e| e.to_string())?;
                let node = card
                    .id
                    .rsplit(':')
                    .next()
                    .and_then(|n| n.parse::<usize>().ok())
                    .ok_or("Invalid mistake identity")?;
                let evidence = source.nodes[node]
                    .evidence
                    .as_ref()
                    .ok_or("Missing mistake evidence")?;
                serde_json::from_value::<crate::engine::Search>(evidence["search"].clone()).ok().and_then(|search|crate::coach_voice::message(&source,node,evidence["label"].as_str(),&search,if de{"de"}else{"en"}).ok()).unwrap_or_else(||if de{"Dieser Zug hat Vorteil verloren. Vergleiche die bessere Fortsetzung mit der stärksten Antwort deines Gegners."}else{"This move lost ground. Compare the better continuation with your opponent's strongest reply."}.into())
            } else {
                if de{"Prüfe Schachgebote, Schlagzüge und Drohungen. Berechne die stärkste Antwort, bevor du ziehst."}else{"Look for checks, captures and threats. Calculate the strongest reply before you commit."}.into()
            };
            let s = Session {
                id: uuid::Uuid::new_v4().to_string(),
                card,
                moves: vec![],
                assisted: false,
                failed: false,
                complete: false,
                recorded: false,
            };
            store_session(&db, &s)?;
            response(&s)
        }
        "move" | "hint" | "skip" | "solution" => {
            let id = args["session"].as_str().ok_or("Session required")?;
            let data: String = db
                .query_row(
                    "SELECT data FROM learning_sessions WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )
                .map_err(|_| "Practice session expired")?;
            let mut s: Session = serde_json::from_str(&data).map_err(|e| e.to_string())?;
            if args["cursor"].as_u64() != Some(s.moves.len() as u64) {
                return Err("Stale practice input".into());
            }
            if action == "solution" {
                if !s.recorded {
                    return Err("Finish or reveal the exercise first".into());
                }
                let mut frames = vec![];
                for n in 0..=s.card.solution.len() {
                    frames.push(
                        play(&s.card.fen, &s.card.solution[..n], s.card.chess960, &s.id)?
                            .snapshot(None)?,
                    );
                }
                return Ok(json!({"frames":frames}));
            }
            if s.complete || s.recorded {
                return response(&s);
            }
            if action == "hint" {
                s.assisted = true;
                store_session(&db, &s)?;
                let mut out = response(&s)?;
                out["hint"] = json!(s.card.solution[s.moves.len()]);
                return Ok(out);
            }
            if action == "skip" {
                s.failed = true;
                record(&db, &mut s)?;
                store_session(&db, &s)?;
                let mut out = response(&s)?;
                out["solution"] = json!(s.card.solution);
                return Ok(out);
            }
            let uci = args["uci"].as_str().ok_or("Move required")?;
            let g = play(&s.card.fen, &s.moves, s.card.chess960, &s.id)?;
            let pos = position(&g.nodes[g.live].fen, s.card.chess960)?;
            let m = uci
                .parse::<UciMove>()
                .map_err(|e| e.to_string())?
                .to_move(&pos)
                .map_err(|e| e.to_string())?;
            let mut end = pos.clone();
            end.play_unchecked(m);
            let alternate_mate = s.card.game_id.is_none()
                && s.card.themes.iter().any(|t| t == "mateIn1")
                && end.is_checkmate();
            if uci != s.card.solution[s.moves.len()] && !alternate_mate {
                s.failed = true;
                store_session(&db, &s)?;
                let mut out = response(&s)?;
                out["correct"] = json!(false);
                return Ok(out);
            }
            s.moves.push(uci.into());
            let first = play(&s.card.fen, &s.moves, s.card.chess960, &s.id)?.snapshot(None)?;
            if !alternate_mate && s.moves.len() < s.card.solution.len() {
                s.moves.push(s.card.solution[s.moves.len()].clone());
            }
            s.complete = alternate_mate || s.moves.len() >= s.card.solution.len();
            if s.complete {
                record(&db, &mut s)?;
            }
            store_session(&db, &s)?;
            let mut out = response(&s)?;
            out["correct"] = json!(true);
            out["player_snapshot"] = json!(first);
            Ok(out)
        }
        _ => Err("Unknown learning action".into()),
    })();
    if result.is_ok() {
        db.commit().map_err(|e| e.to_string())?;
    }
    result
}

fn studies(a: &App, action: &str, args: &Value) -> Result<Value, String> {
    let g = a.game.lock().unwrap().clone();
    let id = args["game_id"].as_str().ok_or("Game required")?;
    if id != g.id {
        return Err("Stale study game".into());
    }
    let db = a.db.lock().unwrap();
    schema(&db)?;
    match action {
        "study_list" => {
            let mut q=db.prepare("SELECT id,origin,fen,moves,note,updated FROM study_variations WHERE game_id=?1 ORDER BY updated DESC").map_err(|e|e.to_string())?;
            let rows = q
                .query_map([id], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, usize>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, u64>(5)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            let mut items = vec![];
            for r in rows {
                let (key, origin, fen, moves, note, updated) = r.map_err(|e| e.to_string())?;
                let moves: Vec<String> = serde_json::from_str(&moves).map_err(|e| e.to_string())?;
                let valid = g.nodes.get(origin).is_some_and(|n| n.fen == fen);
                let sans = if valid {
                    let p = crate::workflows::variation_game(&g, origin, &moves)?;
                    p.path(p.live)
                        .into_iter()
                        .skip_while(|n| *n != origin)
                        .skip(1)
                        .map(|n| p.nodes[n].san.clone())
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                };
                items.push(json!({"id":key,"origin":origin,"moves":moves,"sans":sans,"note":note,"updated":updated,"valid":valid}));
            }
            Ok(json!(items))
        }
        "study_save" => {
            let origin = args["origin"].as_u64().ok_or("Origin required")? as usize;
            let moves: Vec<String> =
                serde_json::from_value(args["moves"].clone()).map_err(|e| e.to_string())?;
            if moves.is_empty() || moves.len() > 500 {
                return Err("Save a variation of 1–500 moves".into());
            }
            crate::workflows::variation_game(&g, origin, &moves)?;
            let note = args["note"].as_str().unwrap_or("");
            if note.len() > 10000 {
                return Err("Note too long".into());
            }
            use sha2::{Digest, Sha256};
            let key = format!(
                "{:x}",
                Sha256::digest(format!(
                    "{}:{}:{}",
                    g.id,
                    g.nodes[origin].fen,
                    moves.join(" ")
                ))
            );
            db.execute(
                "INSERT OR REPLACE INTO study_variations VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    key,
                    g.id,
                    origin,
                    g.nodes[origin].fen,
                    serde_json::to_string(&moves).unwrap(),
                    note,
                    now()
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({"id":key}))
        }
        "study_delete" => {
            db.execute(
                "DELETE FROM study_variations WHERE id=?1 AND game_id=?2",
                params![args["id"].as_str().ok_or("Study required")?, g.id],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!(true))
        }
        _ => Err("Unknown study action".into()),
    }
}

fn threat(a: &App, args: &Value) -> Result<Value, String> {
    let g = a.game.lock().unwrap().clone();
    if args["game_id"].as_str() != Some(&g.id) || args["revision"].as_u64() != Some(g.revision) {
        return Err("Stale threat position".into());
    }
    let node = args["node"].as_u64().ok_or("Node required")? as usize;
    if node >= g.nodes.len() {
        return Err("Unknown position".into());
    }
    let search = crate::run_engine(a, &g, node, false, 250, 1, None)?;
    let mut pos = position(&g.nodes[node].fen, g.chess960)?;
    let mut moves = vec![];
    let mut frames: Vec<Snapshot> = vec![];
    let mut captures = vec![];
    let mut check = false;
    let mut mate = false;
    for u in search
        .lines
        .first()
        .map(|l| l.pv.as_slice())
        .unwrap_or(&[])
        .iter()
        .take(6)
    {
        let m = u
            .parse::<UciMove>()
            .map_err(|e| e.to_string())?
            .to_move(&pos)
            .map_err(|e| e.to_string())?;
        if m.is_capture() {
            captures.push(json!({"ply":moves.len()+1,"role":m.capture().map(|r|r.char().to_string()),"color":if pos.turn()==Color::White{"w"}else{"b"}}))
        }
        pos.play_unchecked(m);
        if moves.is_empty() {
            check = pos.is_check()
        }
        mate = pos.is_checkmate();
        moves.push(u.clone());
        let mut preview = crate::workflows::variation_game(&g, node, &moves)?;
        frames.push(preview.snapshot(None)?);
        if pos.legal_moves().is_empty() {
            break;
        }
    }
    let side = if position(&g.nodes[node].fen, g.chess960)?.turn() == Color::White {
        "w"
    } else {
        "b"
    };
    let de = args["language"].as_str() == Some("de");
    let text = if mate {
        if de {
            "Diese berechnete Fortsetzung endet mit Schachmatt. Schau dir an, wie der König eingeengt wird."
        } else {
            "This calculated continuation ends in checkmate. Watch how the king runs out of squares."
        }
    } else if check {
        if de {
            "Hier beginnt die Fortsetzung mit einem Schach. Prüfe die Antwort und die folgenden Drohungen."
        } else {
            "This continuation starts with check. Check the reply and the threats that follow."
        }
    } else if !captures.is_empty() {
        if de {
            "In dieser Fortsetzung werden Figuren geschlagen. Achte darauf, welche Figuren angegriffen werden und ob zurückgeschlagen werden kann."
        } else {
            "There are captures in this continuation. Watch which pieces come under attack and whether they can be recaptured."
        }
    } else {
        if de {
            "Es gibt hier keine belegte unmittelbare Schlagdrohung. Schau dir diese berechnete Fortsetzung an und prüfe die Möglichkeiten beider Seiten."
        } else {
            "There is no verified immediate capture threat here. Explore this calculated continuation and check both sides' options."
        }
    };
    let current = a.game.lock().unwrap();
    if current.id != g.id || current.revision != g.revision {
        return Err("Position changed during threat analysis".into());
    }
    Ok(
        json!({"game_id":g.id,"revision":g.revision,"node":node,"side":side,"text":text,"moves":moves,"frames":frames,"captures":captures,"depth":search.lines.first().map(|l|l.depth),"elapsed_ms":search.elapsed_ms,"note":"A legal engine continuation, not a guarantee that the opponent must play it."}),
    )
}

#[tauri::command]
pub async fn learning(app: tauri::AppHandle, action: String, args: Value) -> Result<Value, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || dispatch(&app.state::<App>(), &action, &args))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_bundled_puzzles_have_legal_setup_and_complete_solutions() {
        let p = corpus().unwrap();
        assert_eq!(p.len(), 10800);
        assert!(p.iter().any(|p| p.rating < 800));
        assert!(p.iter().any(|p| p.rating > 2400));
        let mut ids = HashSet::new();
        for x in p {
            assert!(ids.insert(&x.id));
            let c = puzzle_card(x).unwrap();
            assert_eq!(c.solution, x.moves[1..]);
            play(&c.fen, &c.solution, false, "test").unwrap();
        }
    }
    #[test]
    fn repetition_requires_clean_success_and_schedules_revisits() {
        let p = schedule(Progress::default(), true, 100);
        assert_eq!(p.due, 86500);
        let p = schedule(p, true, 200);
        assert_eq!(p.interval, 259200);
        let p = schedule(p, false, 300);
        assert_eq!(p.streak, 0);
        assert_eq!(p.due, 900);
        assert_eq!(p.wins, 2);
        assert_eq!(p.attempts, 3);
    }
    #[test]
    fn learning_schema_is_additive_and_idempotent() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE games(id TEXT,data TEXT);INSERT INTO games VALUES('keep','original');",
        )
        .unwrap();
        schema(&db).unwrap();
        schema(&db).unwrap();
        assert_eq!(
            db.query_row("SELECT data FROM games", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "original"
        );
    }
    fn app() -> App {
        use std::sync::{atomic::AtomicU64, Mutex};
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE games(id TEXT PRIMARY KEY,name TEXT,data TEXT);CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT);").unwrap();
        App {
            review_ticket: AtomicU64::new(0),
            game: Mutex::new(
                Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap(),
            ),
            db: Mutex::new(db),
            analyst: Mutex::new(None),
            opponent: Mutex::new(None),
            engine_path: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources/stockfish-windows-x86-64-universal.exe"),
            threads: 2,
            hash: 64,
            language: Mutex::new(crate::language::Runtime::default()),
        }
    }
    #[test]
    fn wrong_and_stale_practice_input_do_not_advance_or_modify_original() {
        let a = app();
        let original = serde_json::to_string(&*a.game.lock().unwrap()).unwrap();
        let mut r = dispatch(&a, "start", &json!({"rating":600})).unwrap();
        let id = r["session"].clone();
        let c = r["card"]["id"]
            .as_str()
            .unwrap()
            .strip_prefix("puzzle:")
            .unwrap();
        let p = corpus().unwrap().iter().find(|p| p.id == c).unwrap();
        let wrong = r["snapshot"]["legal"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|u| u.as_str())
            .find(|u| *u != p.moves[1])
            .unwrap()
            .to_owned();
        r = dispatch(&a, "move", &json!({"session":id,"cursor":0,"uci":wrong})).unwrap();
        assert_eq!(r["correct"], false);
        assert_eq!(r["cursor"], 0);
        assert!(dispatch(
            &a,
            "move",
            &json!({"session":id,"cursor":1,"uci":p.moves[1]})
        )
        .is_err());
        r = dispatch(
            &a,
            "move",
            &json!({"session":id,"cursor":0,"uci":p.moves[1]}),
        )
        .unwrap();
        assert!(r["cursor"].as_u64().unwrap() > 0);
        assert_eq!(
            serde_json::to_string(&*a.game.lock().unwrap()).unwrap(),
            original
        );
    }
    #[test]
    fn hint_assisted_complete_records_once_without_clean_win() {
        let a = app();
        let mut r = dispatch(&a, "start", &json!({"rating":600})).unwrap();
        let id = r["session"].clone();
        let card = r["card"]["id"].as_str().unwrap().to_owned();
        let p = corpus()
            .unwrap()
            .iter()
            .find(|p| format!("puzzle:{}", p.id) == card)
            .unwrap();
        dispatch(&a, "hint", &json!({"session":id,"cursor":0})).unwrap();
        while r["complete"] != true {
            let cursor = r["cursor"].as_u64().unwrap() as usize;
            r = dispatch(
                &a,
                "move",
                &json!({"session":id,"cursor":cursor,"uci":p.moves[cursor+1]}),
            )
            .unwrap();
        }
        dispatch(
            &a,
            "move",
            &json!({"session":id,"cursor":r["cursor"],"uci":"bad"}),
        )
        .unwrap();
        let db = a.db.lock().unwrap();
        let pr = progress(&db, &card);
        assert_eq!(pr.attempts, 1);
        assert_eq!(pr.wins, 0);
        assert_eq!(pr.streak, 0);
    }
    #[test]
    fn saved_variations_validate_reopen_notes_and_preserve_game() {
        let a = app();
        let g = a.game.lock().unwrap().clone();
        let before = serde_json::to_string(&g).unwrap();
        let args = json!({"game_id":g.id,"origin":0,"moves":["e2e4","e7e5","g1f3"],"note":"My plan\nKeep the bishop active."});
        dispatch(&a, "study_save", &args).unwrap();
        dispatch(&a, "study_save", &args).unwrap();
        let rows = dispatch(&a, "study_list", &json!({"game_id":g.id})).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["sans"], json!(["e4", "e5", "Nf3"]));
        assert_eq!(rows[0]["note"], args["note"]);
        assert!(dispatch(
            &a,
            "study_save",
            &json!({"game_id":g.id,"origin":0,"moves":["e2e5"]})
        )
        .is_err());
        assert_eq!(
            serde_json::to_string(&*a.game.lock().unwrap()).unwrap(),
            before
        );
        dispatch(
            &a,
            "study_delete",
            &json!({"game_id":g.id,"id":rows[0]["id"]}),
        )
        .unwrap();
        assert!(dispatch(&a, "study_list", &json!({"game_id":g.id}))
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn alternate_legal_mate_in_one_is_accepted() {
        let a = app();
        let fen = "7k/5K2/8/6Q1/8/8/8/8 w - - 0 1";
        let s = Session {
            id: "alternate-mate".into(),
            card: Card {
                id: "puzzle:test".into(),
                fen: fen.into(),
                solution: vec!["g5g7".into()],
                rating: 600,
                themes: vec!["mateIn1".into()],
                title: "Mate".into(),
                explanation: "Checkmate".into(),
                source: "Test".into(),
                game_id: None,
                node: None,
                chess960: false,
            },
            moves: vec![],
            assisted: false,
            failed: false,
            complete: false,
            recorded: false,
        };
        {
            let db = a.db.lock().unwrap();
            schema(&db).unwrap();
            store_session(&db, &s).unwrap();
        }
        let result =
            dispatch(&a, "move", &json!({"session":s.id,"cursor":0,"uci":"g5h5"})).unwrap();
        assert_eq!(result["correct"], true);
        assert_eq!(result["complete"], true);
        assert!(result["snapshot"]["status"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("checkmate"));
    }
    #[test]
    fn mistakes_only_come_from_own_played_path_and_verified_legal_repairs() {
        let a = app();
        let mut g = a.game.lock().unwrap().clone();
        g.commit("e2e4", g.revision, false).unwrap();
        g.nodes[1].evidence = Some(
            json!({"label":"Mistake","best_alternative":"d4","detail":"Improve development."}),
        );
        g.commit("e7e5", g.revision, false).unwrap();
        g.nodes[2].evidence = Some(json!({"label":"Blunder","best_alternative":"c7c5"}));
        let db = a.db.lock().unwrap();
        db.execute(
            "INSERT INTO games VALUES(?1,?2,?3)",
            params![g.id, g.name, serde_json::to_string(&g).unwrap()],
        )
        .unwrap();
        let cards = mistakes(&db).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].solution, vec!["d2d4"]);
        assert_eq!(cards[0].fen, crate::game::START);
    }
    #[test]
    fn all_scoped_mistakes_remain_reachable_beyond_thirty_cards() {
        let a = app();
        {
            let db = a.db.lock().unwrap();
            for n in 0..36 {
                let mut g =
                    Game::new(crate::game::START.into(), "w".into(), 400, 0., 0., false).unwrap();
                g.id = format!("scoped-{n}");
                g.name = format!("Game {n}");
                g.commit("e2e4", g.revision, false).unwrap();
                g.nodes[1].evidence = Some(json!({"label":"Mistake","best_alternative":"d4"}));
                db.execute(
                    "INSERT INTO games VALUES(?1,?2,?3)",
                    params![g.id, g.name, serde_json::to_string(&g).unwrap()],
                )
                .unwrap();
            }
        }
        let o = dispatch(&a, "profile", &json!({})).unwrap();
        assert_eq!(o["cards"].as_array().unwrap().len(), 36);
        assert!(o["cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["game_id"] == "scoped-35"));
        let p = dispatch(
            &a,
            "start",
            &json!({"kind":"mistakes","game_id":"scoped-35"}),
        )
        .unwrap();
        assert_eq!(p["card"]["game_id"], "scoped-35");
    }
}
