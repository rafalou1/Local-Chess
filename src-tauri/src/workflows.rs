use crate::{
    game::{position, Game, Snapshot},
    pgn, save, App,
};
use serde_json::{json, Value};
use tauri::State;
#[tauri::command]
pub async fn save_export(
    app: tauri::AppHandle,
    a: State<'_, App>,
    format: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let g = a.game.lock().unwrap().clone();
    if !["pgn", "json"].contains(&format.as_str()) {
        return Err("Choose PGN or JSON export".into());
    }
    let data = if format == "json" {
        serde_json::to_string_pretty(&g).map_err(|e| e.to_string())?
    } else {
        pgn::export(&g)
    };
    let path = app
        .dialog()
        .file()
        .set_file_name(format!("local-chess-{}.{}", g.id, format))
        .add_filter(format.to_uppercase(), &[format.as_str()])
        .blocking_save_file();
    let Some(path) = path else { return Ok(None) };
    let path = path
        .into_path()
        .map_err(|_| "Only local filesystem exports are supported")?;
    std::fs::write(&path, data).map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into()))
}

fn review_signature(g: &Game, language: &str) -> String {
    use sha2::{Digest, Sha256};
    let moves: Vec<_> = g
        .path(g.live)
        .iter()
        .map(|&n| (&g.nodes[n].fen, &g.nodes[n].uci))
        .collect();
    format!(
        "coach-v14-contextual-review-arrows:{language}:{:x}",
        Sha256::digest(serde_json::to_vec(&moves).unwrap())
    )
}
#[tauri::command]
pub fn cancel_review(a: State<App>) {
    a.review_ticket
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}
/// Replay an engine line in a temporary game. No live node, clock, or DB row is changed.
pub(crate) fn variation_game(
    original: &Game,
    node: usize,
    moves: &[String],
) -> Result<Game, String> {
    if node >= original.nodes.len() {
        return Err("Invalid variation origin".into());
    }
    let mut preview = original.clone();
    preview.live = node;
    preview.result = None;
    preview.paused = false;
    preview.human = "both".into();
    preview.base_ms = 0.;
    preview.increment_ms = 0.;
    preview.tick = None;
    // Analysis continues through claimable/repetition/material draws. A mate or
    // stalemate still has no legal continuation, so native legality rejects it.
    for uci in moves {
        preview.result = None;
        let rev = preview.revision;
        preview.commit(uci, rev, false)?;
    }
    preview.revision = original.revision;
    Ok(preview)
}
#[cfg(test)]
mod variation_tests {
    use super::*;
    fn game(fen: &str) -> Game {
        Game::new(fen.into(), "w".into(), 400, 300000., 2000., false).unwrap()
    }
    fn line(g: &Game, node: usize, moves: &[&str]) -> Result<Game, String> {
        variation_game(
            g,
            node,
            &moves.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
    }
    #[test]
    fn manual_line_isolated_from_paused_finished_recording() {
        let mut g = game(crate::game::START);
        g.paused = false;
        g.commit("e2e4", g.revision, false).unwrap();
        g.pause();
        g.result = Some("White wins".into());
        let before = serde_json::to_value(&g).unwrap();
        let mut p = line(&g, 1, &["c7c5", "g1f3", "d7d6", "d2d4", "c5d4"]).unwrap();
        let s = p.snapshot(None).unwrap();
        assert_eq!(s.turn, "w");
        assert_eq!(s.game.human, "both");
        assert_eq!(s.game.base_ms, 0.);
        assert_eq!(p.revision, g.revision);
        assert_eq!(serde_json::to_value(&g).unwrap(), before);
        assert!(line(&g, 1, &["e2e3"]).is_err());
        assert!(line(&g, 999, &[]).is_err());
    }
    #[test]
    fn variation_castling_en_passant_promotion_and_terminal() {
        let castle = game("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        let p = line(&castle, 0, &["e1g1", "e8c8"]).unwrap();
        assert!(p.nodes[p.live].fen.starts_with("2kr3r/"));
        let ep = game("7k/3p4/8/4P3/8/8/8/K7 b - - 0 1");
        let p = line(&ep, 0, &["d7d5", "e5d6"]).unwrap();
        assert_eq!(p.nodes[p.live].san, "exd6");
        let promotion = game("7k/P7/8/8/8/8/8/K7 w - - 0 1");
        assert!(line(&promotion, 0, &["a7a8"]).is_err());
        assert!(line(&promotion, 0, &["a7a8n"])
            .unwrap()
            .nodes
            .last()
            .unwrap()
            .san
            .starts_with("a8=N"));
        let mate = game(crate::game::START);
        let p = line(&mate, 0, &["f2f3", "e7e5", "g2g4", "d8h4"]).unwrap();
        assert!(p.result.is_some());
        assert!(line(&mate, 0, &["f2f3", "e7e5", "g2g4", "d8h4", "a2a3"]).is_err());
    }
    #[test]
    fn variation_can_exceed_old_twelve_ply_preview_limit() {
        let mut g = game(crate::game::START);
        g.base_ms = 0.;
        g.paused = false;
        let moves = [
            "e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "g8f6", "d2d3", "f8c5", "c2c3", "d7d6", "e1g1",
            "e8g8", "b1d2", "a7a6", "a2a4", "c8e6",
        ];
        let p = line(&g, 0, &moves).unwrap();
        assert_eq!(p.path(p.live).len(), 17);
        assert_eq!(g.nodes.len(), 1);
    }
    #[test]
    fn analysis_continues_after_repetition_or_material_draw() {
        let g = game(crate::game::START);
        let moves = ["g1f3", "g8f6", "f3g1", "f6g8"].repeat(8);
        let p = line(&g, 0, &moves).unwrap();
        assert_eq!(p.path(p.live).len(), 33);
        let g = game("7k/P7/8/8/8/8/8/K7 w - - 0 1");
        assert!(line(&g, 0, &["a7a8n", "h8g8", "a8b6"]).is_ok());
    }
    #[test]
    fn hypothetical_coaching_keeps_recorded_player_identity_in_both_languages() {
        for human in ["w", "b", "both"] {
            let mut original=game(crate::game::START);
            original.human=human.into();
            let preview=line(&original,0,&["e2e4","d7d5","e4e5","g8f6"]).unwrap();
            let pv=vec!["e5f6".to_string(),"g7f6".to_string()];
            let san=preview.legal_line(preview.live,&pv).unwrap();
            let search=crate::engine::Search {lines:vec![crate::engine::Line {depth:20,cp:Some(300),pv,san,..Default::default()}],best:"e5f6".into(),elapsed_ms:180.,engine:"Stockfish 19".into()};
            let voice=review_voice_position(&original,&preview);
            assert_eq!(preview.human,"both","Both sides must stay movable");
            assert_eq!(voice.human,original.human,"Voice retains the recorded player");
            for language in ["en","de"] {
                let text=crate::coach_answers::friendly(&voice,voice.live,Some("Blunder"),None,&search,language).unwrap();
                let expected=match (human=="b",language) {
                    (true,"en")=>"your knight",(false,"en")=>"my knight",
                    (true,_)=>"deinen Springer",(false,_)=>"meinen Springer",
                };
                assert!(text.contains(expected),"{human}/{language}: {text}");
            }
        }
    }
}
fn review_voice_position(original: &Game, preview: &Game) -> Game {
    let mut voice = preview.clone();
    // Both-side board input must not change which side the coach calls mine/yours.
    voice.human = original.human.clone();
    voice
}

/// Input acknowledgement has no engine wait and never writes the original game.
#[tauri::command]
pub fn review_variation(
    a: State<App>,
    game_id: String,
    revision: u64,
    node: usize,
    moves: Vec<String>,
) -> Result<Snapshot, String> {
    let original = a.game.lock().unwrap();
    if original.id != game_id || original.revision != revision {
        return Err("Stale variation".into());
    }
    variation_game(&original, node, &moves)?.snapshot(None)
}
#[tauri::command]
pub async fn review_line(
    app: tauri::AppHandle,
    game_id: String,
    revision: u64,
    node: usize,
    moves: Vec<String>,
    classify: Option<bool>,
) -> Result<Value, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let a = app.state::<App>();
        let original = a.game.lock().unwrap().clone();
        if original.id != game_id || original.revision != revision { return Err("Stale or invalid review line".into()); }
        let mut preview = variation_game(&original, node, &moves)?;
        let search = crate::run_engine(&a, &preview, preview.live, false, 180, 1, None)?;
        let analysis = if classify.unwrap_or(false) && !moves.is_empty() {
            let at = preview.live;
            let parent = preview.nodes[at].parent.unwrap();
            let before = crate::run_engine(&a, &preview, parent, false, 180, 3, None)?;
            let played = if let Some(candidate) = crate::same_root_candidate(&preview,at,&before) {
                candidate
            } else { crate::run_engine(&a, &preview, parent, false, 180, 1, Some(&preview.nodes[at].uci))? };
            let (label,loss,alternative) = crate::decision_from_search(&a,&preview,at,&before,&played,300)?;
            let voice_position = review_voice_position(&original,&preview);
            let language = crate::coach_answers::language(&a);
            let explanation = crate::coach_answers::friendly(&voice_position,at,label.as_deref(),alternative.as_deref(),&search,&language)?;
            Some(json!({"label":label,"loss":loss,"best_alternative":alternative,"explanation":explanation}))
        } else { None };
        {let current=a.game.lock().unwrap();if current.id!=game_id||current.revision!=revision{return Err("Stale review line discarded".into());}}
        Ok(json!({"snapshot":preview.snapshot(None)?,"search":search,"analysis":analysis}))
    }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub fn review_report(a: State<App>) -> Result<Option<Value>, String> {
    let g = a.game.lock().unwrap().clone();
    let signature = review_signature(&g, &crate::coach_answers::language(&a));
    let data: Option<String> =
        a.db.lock()
            .unwrap()
            .query_row(
                "SELECT value FROM settings WHERE key=?1",
                [format!("review:{}", g.id)],
                |r| r.get(0),
            )
            .ok();
    Ok(data
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(|r| r["signature"] == signature))
}
#[tauri::command]
pub async fn review_game(
    app: tauri::AppHandle,
    game_id: String,
    revision: u64,
    request_id: Option<String>,
) -> Result<Value, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let a = app.state::<App>();
        review_impl(&app, &a, game_id, revision, request_id)
    })
    .await
    .map_err(|e| e.to_string())?
}
fn review_impl(
    app: &tauri::AppHandle,
    a: &App,
    game_id: String,
    revision: u64,
    request_id: Option<String>,
) -> Result<Value, String> {
    use shakmaty::{uci::UciMove, Color, Position};
    use std::sync::atomic::Ordering;
    use tauri::Emitter;
    let mut g = a.game.lock().unwrap().clone();
    if g.id != game_id || g.revision != revision {
        return Err("Stale review request".into());
    }
    let language = crate::coach_answers::language(a);
    let signature = review_signature(&g, &language);
    let cached: Option<String> =
        a.db.lock()
            .unwrap()
            .query_row(
                "SELECT value FROM settings WHERE key=?1",
                [format!("review:{}", g.id)],
                |r| r.get(0),
            )
            .ok();
    if let Some(r) = cached
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(|r| r["signature"] == signature)
    {
        return Ok(r);
    }
    let ticket = a.review_ticket.fetch_add(1, Ordering::SeqCst) + 1;
    let check = || -> Result<(), String> {
        if a.review_ticket.load(Ordering::SeqCst) != ticket {
            return Err("Review cancelled".into());
        }
        let current = a.game.lock().unwrap();
        if current.id != game_id || current.revision != revision {
            return Err("Stale review cancelled".into());
        }
        Ok(())
    };
    let path = g.path(g.live);
    let began = std::time::Instant::now();
    let mut counts = [
        std::collections::BTreeMap::<String, u32>::new(),
        std::collections::BTreeMap::<String, u32>::new(),
    ];
    let mut accuracy_samples = [Vec::<f64>::new(), Vec::<f64>::new()];
    let mut moves = vec![];
    let mut root = None;
    let mut previous: Option<crate::engine::Search> = None;
    let (mut position_searches, mut restricted_searches, mut reused_candidate_lines) = (0, 0, 0);
    // One unrestricted search per actual position. Its exact legal root candidates
    // are reused for the next decision, rather than searching the same parent twice.
    // Unrestricted means opponent Elo never enters this analysis.
    for (index, &node) in path.iter().enumerate() {
        check()?;
        let pos = position(&g.nodes[node].fen, g.chess960)?;
        let search = if pos.legal_moves().is_empty() || pos.is_insufficient_material() {
            crate::engine::Search {
                lines: vec![],
                best: String::new(),
                elapsed_ms: 0.,
                engine: "Stockfish 19".into(),
            }
        } else {
            position_searches += 1;
            crate::run_engine(a, &g, node, false, 170, 3, None)?
        };
        check()?;
        let white = if pos.is_checkmate() {
            Some(json!({"cp":if pos.turn()==Color::White{-100000}else{100000},"mate":0,"depth":0}))
        } else if pos.is_stalemate() || pos.is_insufficient_material() {
            Some(json!({"cp":0,"mate":null,"depth":0}))
        } else {
            search
                .lines
                .first()
                .map(|l| json!({"cp":l.cp,"mate":l.mate,"depth":l.depth}))
        };
        if index == 0 {
            root = white;
            previous = Some(search);
            let _=app.emit("review-update",json!({"game_id":g.id,"revision":g.revision,"request_id":request_id,"version":6,"complete":false,"total":path.len()-1,"counts":{"w":counts[0],"b":counts[1]},"accuracy":{"w":null,"b":null},"accuracy_model":"Mean 100 × exp(−centipawn loss / 200), local v2; not Chess.com accuracy","turning_points":[],"analyzed":0,"root":root,"moves":[],"elapsed_ms":began.elapsed().as_millis()}));
            continue;
        }
        let parent = g.nodes[node].parent.unwrap();
        let best = previous.as_ref().unwrap();
        let played = if let Some(candidate)=crate::same_root_candidate(&g,node,best) {
            reused_candidate_lines += 1;
            candidate
        } else if let Some(child_root)=crate::played_from_child(&g,node,&search) {
            reused_candidate_lines += 1;
            child_root
        } else {
            restricted_searches += 1;
            crate::run_engine(a, &g, parent, false, 170, 1, Some(&g.nodes[node].uci))?
        };
        check()?;
        let resolved=crate::grading::resolve(a,&g,node,best,&played)?;
        let (label, loss, alternative) = resolved.decision.clone();
        let side = if position(&g.nodes[parent].fen, g.chess960)?.turn() == Color::White {
            0
        } else {
            1
        };
        let cp_loss = review_cp_loss(resolved.best.lines.first(), resolved.played.lines.first(), side == 0);
        // Accuracy and tactical practice retain raw centipawn loss. Feedback
        // shares the live model; review must not secretly apply a harsher scale.
        if let Some(cp) = cp_loss {
            accuracy_samples[side].push(100. * (-cp / 200.).exp());
        }
        let mut facts = crate::coaching::consequence(&g, node)?;
        facts.extend(crate::coaching::line_facts(&g, node, &search)?);
        let ray = crate::coach_answers::verified_ray(&g, node, language == "de")?;
        let explanation = crate::coach_answers::friendly(
            &g,
            node,
            label.as_deref(),
            alternative.as_deref(),
            &search,
            &language,
        )?;
        let detail = if matches!(
            label.as_deref(),
            Some("Inaccuracy" | "Mistake" | "Blunder" | "Miss")
        ) {
            crate::coach_answers::mistake_reason(
                &g,
                node,
                alternative.as_deref(),
                &search,
                &language,
            )?
        } else {
            facts.join(" ")
        };
        let reply = search
            .lines
            .first()
            .and_then(|l| l.pv.first())
            .filter(|u| {
                u.parse::<UciMove>()
                    .ok()
                    .and_then(|u| u.to_move(&pos).ok())
                    .is_some()
            })
            .cloned();
        let target = reply.as_ref().and_then(|u| {
            let m = u.parse::<UciMove>().ok()?.to_move(&pos).ok()?;
            if let shakmaty::Move::EnPassant { to, .. } = m {
                Some(shakmaty::Square::new((to as u32) ^ 8).to_string())
            } else {
                m.is_capture().then(|| u[2..4].to_owned())
            }
        });
        if let Some(ref name) = label {
            *counts[side].entry(name.clone()).or_default() += 1
        }
        g.nodes[node].evidence = Some(
            json!({"search":search,"label":label,"loss":loss,"cp_loss":cp_loss,"best_alternative":alternative,"explanation":explanation,"detail":detail,"facts":facts}),
        );
        moves.push(json!({"node":node,"parent":parent,"color":if side==0{"w"}else{"b"},"san":g.nodes[node].san,"uci":g.nodes[node].uci,"label":label,"loss":loss,"cp_loss":cp_loss,"best_alternative":alternative,"best_move":best.best,"explanation":explanation,"detail":detail,"search":search,"evaluation":white,"reply":reply,"target":target,"motif_arrow":ray.as_ref().map(|r|&r.0),"motif_target":ray.as_ref().map(|r|&r.1),"facts":facts,"move_ms":if side==0{g.nodes[parent].white_ms+g.increment_ms-g.nodes[node].white_ms}else{g.nodes[parent].black_ms+g.increment_ms-g.nodes[node].black_ms}}));
        previous = Some(search);
        let avg = |v: &Vec<f64>| {
            if v.is_empty() {
                None
            } else {
                Some(v.iter().sum::<f64>() / v.len() as f64)
            }
        };
        check()?;
        let _=app.emit("review-update",json!({"game_id":g.id,"revision":g.revision,"request_id":request_id,"version":6,"complete":false,"total":path.len()-1,"counts":{"w":counts[0],"b":counts[1]},"accuracy":{"w":avg(&accuracy_samples[0]),"b":avg(&accuracy_samples[1])},"accuracy_model":"Mean 100 × exp(−centipawn loss / 200), local v2; not Chess.com accuracy","turning_points":[],"analyzed":moves.len(),"root":root,"moves":moves,"elapsed_ms":began.elapsed().as_millis()}));
        let _=app.emit("review-progress",json!({"game_id":g.id,"revision":g.revision,"done":index,"total":path.len()-1,"elapsed_ms":began.elapsed().as_millis()}));
    }
    let accuracy = |v: &Vec<f64>| {
        if v.is_empty() {
            None
        } else {
            Some(v.iter().sum::<f64>() / v.len() as f64)
        }
    };
    let mut turning: Vec<_> = moves
        .iter()
        .filter(|m| {
            m["loss"].as_f64().unwrap_or(0.) >= 0.05 || m["cp_loss"].as_f64().unwrap_or(0.) >= 75.
        })
        .cloned()
        .collect();
    turning.sort_by(|a, b| {
        b["cp_loss"]
            .as_f64()
            .unwrap_or(0.)
            .total_cmp(&a["cp_loss"].as_f64().unwrap_or(0.))
    });
    let report = json!({"game_id":g.id,"signature":signature,"version":6,"complete":true,"total":path.len()-1,"counts":{"w":counts[0],"b":counts[1]},"accuracy":{"w":accuracy(&accuracy_samples[0]),"b":accuracy(&accuracy_samples[1])},"accuracy_model":"Mean 100 × exp(−centipawn loss / 200), local v2; not Chess.com accuracy","turning_points":turning.into_iter().take(5).collect::<Vec<_>>(),"analyzed":moves.len(),"root":root,"moves":moves,"elapsed_ms":began.elapsed().as_millis(),"work":{"position_searches":position_searches,"restricted_searches":restricted_searches,"reused_candidate_lines":reused_candidate_lines,"reuse_note":"Comparable same-root candidate, quiet child continuation, or material-safe restricted root search","position_budget_ms":170,"special_refinement_ms":300}});
    check()?;
    let mut current = a.game.lock().unwrap();
    if current.id != g.id
        || current.revision != g.revision
        || a.review_ticket.load(Ordering::SeqCst) != ticket
    {
        return Err("Stale review discarded".into());
    }
    // Commit evidence once only after successful completion. Navigation state and
    // clocks stay owned by the current game, not the start-of-review clone.
    for &node in &path {
        if node > 0 {
            current.nodes[node].evidence = g.nodes[node].evidence.clone()
        }
    }
    crate::save(a, &current)?;
    a.db.lock()
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO settings(key,value) VALUES(?1,?2)",
            rusqlite::params![format!("review:{}", g.id), report.to_string()],
        )
        .map_err(|e| e.to_string())?;
    Ok(report)
}
fn review_cp_loss(
    best: Option<&crate::engine::Line>,
    played: Option<&crate::engine::Line>,
    white: bool,
) -> Option<f64> {
    let (b, p) = (best?, played?);
    if b.bound || p.bound || b.depth < 8 || p.depth < 8 {
        return None;
    }
    let score = |l: &crate::engine::Line| {
        l.mate
            .map(|m| if m >= 0 { 100000. } else { -100000. })
            .or(l.cp.map(|cp| cp as f64))
    };
    let delta = (score(b)? - score(p)?) * if white { 1. } else { -1. };
    Some(delta.clamp(0., 2000.))
}

#[tauri::command]
pub fn import_pgn(a: State<App>, data: String) -> Result<Snapshot, String> {
    let games = pgn::import(&data)?;
    let mut g = a.game.lock().unwrap();
    g.pause();
    save(&a, &g)?;
    for imported in &games {
        save(&a, imported)?
    }
    *g = games.into_iter().next().ok_or("No games")?;
    save(&a, &g)?;
    g.snapshot(None)
}
#[tauri::command]
pub fn study(a: State<App>, node: usize, practice: bool) -> Result<Snapshot, String> {
    let mut g = a.game.lock().unwrap();
    if node >= g.nodes.len() {
        return Err("Unknown node".into());
    }
    g.pause();
    save(&a, &g)?;
    let mut next = g.clone();
    next.id = uuid::Uuid::new_v4().to_string();
    next.name = format!("Study: {}", g.name);
    next.live = node;
    next.result = None;
    next.revision += 1;
    next.base_ms = 0.;
    next.human = if practice {
        g.human.clone()
    } else {
        "both".into()
    };
    next.tick = None;
    next.paused = false;
    *g = next;
    save(&a, &g)?;
    g.snapshot(None)
}
#[tauri::command]
pub fn branch_move(
    a: State<App>,
    node: usize,
    revision: u64,
    uci: String,
) -> Result<Snapshot, String> {
    let mut g = a.game.lock().unwrap();
    if g.human != "both" || node >= g.nodes.len() || g.revision != revision {
        return Err("Open a study session before branching".into());
    }
    g.live = node;
    g.result = None;
    g.commit(&uci, revision, false)?;
    save(&a, &g)?;
    g.snapshot(None)
}
#[tauri::command]
pub fn annotate_node(a: State<App>, node: usize, comment: String) -> Result<(), String> {
    let mut g = a.game.lock().unwrap();
    g.nodes.get_mut(node).ok_or("Unknown node")?.comment = comment;
    save(&a, &g)
}
#[tauri::command]
pub fn rename_game(a: State<App>, id: String, name: String) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Name cannot be empty".into());
    }
    let mut g = a.game.lock().unwrap();
    if g.id == id {
        g.name = name;
        save(&a, &g)
    } else {
        let data: String =
            a.db.lock()
                .unwrap()
                .query_row("SELECT data FROM games WHERE id=?1", [&id], |r| r.get(0))
                .map_err(|e| e.to_string())?;
        let mut other: Game = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        other.name = name;
        let mut db = a.db.lock().unwrap();
        let tx = db.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE games SET name=?1,data=?2 WHERE id=?3",
            rusqlite::params![other.name, serde_json::to_string(&other).unwrap(), id],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
}
#[tauri::command]
pub fn training_progress(a: State<App>, key: String, success: bool) -> Result<Value, String> {
    let db = a.db.lock().unwrap();
    let old: String = db
        .query_row("SELECT value FROM settings WHERE key='training'", [], |r| {
            r.get(0)
        })
        .unwrap_or("{}".into());
    let mut v: Value = serde_json::from_str(&old).unwrap_or(json!({}));
    let item = v
        .get(&key)
        .cloned()
        .unwrap_or(json!({"attempts":0,"correct":0}));
    v[&key] = json!({"attempts":item["attempts"].as_u64().unwrap_or(0)+1,"correct":item["correct"].as_u64().unwrap_or(0)+if success{1}else{0}});
    db.execute(
        "INSERT OR REPLACE INTO settings(key,value) VALUES('training',?1)",
        [v.to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(v)
}
#[tauri::command]
pub fn record_metrics(a: State<App>, data: Value) -> Result<(), String> {
    a.db.lock()
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO settings(key,value) VALUES('ui_metrics',?1)",
            [data.to_string()],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
pub async fn benchmark(a: State<'_, App>) -> Result<Value, String> {
    let mut g = a.game.lock().unwrap();
    g.pause();
    save(&a, &g)?;
    drop(g);
    let mut opponent = a.opponent.lock().unwrap();
    let mut analyst = a.analyst.lock().unwrap();
    *opponent = None;
    *analyst = None;
    let suite = [
        crate::game::START,
        "r1bq1rk1/ppp2ppp/2np1n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQ1RK1 w - - 4 7",
        "8/5pk1/6p1/3p4/3P4/5KP1/5P2/8 w - - 0 1",
    ];
    let mut rows = vec![];
    for threads in [2, 6] {
        let mut engine = crate::engine::Engine::new(&a.engine_path, threads, 128)?;
        for multipv in [1, 3, 5] {
            for (index, fen) in suite.iter().enumerate() {
                position(fen, false)?;
                for pass in 0..2 {
                    let search = engine.search(
                        &format!("position fen {fen}"),
                        250,
                        multipv,
                        None,
                        None,
                        false,
                    )?;
                    rows.push(json!({"threads":threads,"multipv":multipv,"position":index,"pass":pass,"elapsed_ms":search.elapsed_ms,"depth":search.lines.first().map(|l|l.depth),"nps":search.lines.first().map(|l|l.nps)}));
                }
            }
        }
    }
    let mut times = rows
        .iter()
        .filter_map(|r| r["elapsed_ms"].as_f64())
        .collect::<Vec<_>>();
    times.sort_by(f64::total_cmp);
    let report = json!({"engine":"Stockfish 19","rows":rows,"p50_ms":times[times.len()/2],"p95_ms":times[(times.len()*95/100).min(times.len()-1)],"method":"Two passes; three positions; MultiPV 1/3/5; threads 2/6; 128 MiB hash; 250 ms requested search. In native application command, board remains available. Opponent and analyst searches stopped; no thermal/soak inference."});
    a.db.lock()
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO settings(key,value) VALUES('benchmark',?1)",
            [report.to_string()],
        )
        .map_err(|e| e.to_string())?;
    Ok(report)
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use crate::engine::Line;
    #[test]
    fn cp_loss_sign_bounds_and_mates() {
        let b = Line {
            depth: 16,
            cp: Some(400),
            ..Default::default()
        };
        let p = Line {
            depth: 16,
            cp: Some(-300),
            ..Default::default()
        };
        assert_eq!(review_cp_loss(Some(&b), Some(&p), true), Some(700.));
        assert_eq!(review_cp_loss(Some(&p), Some(&b), false), Some(700.));
        assert_eq!(review_cp_loss(Some(&b), Some(&p), false), Some(0.));
        let bound = Line {
            bound: true,
            ..b.clone()
        };
        assert_eq!(review_cp_loss(Some(&bound), Some(&p), true), None);
        let mate = Line {
            depth: 18,
            mate: Some(3),
            ..b.clone()
        };
        assert_eq!(review_cp_loss(Some(&mate), Some(&p), true), Some(2000.));
        let still_mate = Line {
            mate: Some(8),
            ..mate.clone()
        };
        assert_eq!(
            review_cp_loss(Some(&mate), Some(&still_mate), true),
            Some(0.)
        );
    }
}
