#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod classification;

mod provider_play;

mod coach_answers;

mod coach_voice;

mod coaching;

#[cfg(debug_assertions)]

mod driver;

mod engine;

mod grading;

mod reference_engine;

mod game;

mod language;

mod learning;

mod local_ai;

mod neural_voice;

mod openings;

mod feedback_model;

mod pgn;

#[cfg(test)]

mod rule_tests;

mod speech;

mod voice_ffi;

mod workflows;

use engine::{Engine, Search};

use game::{Game, Snapshot, START};

use rusqlite::{params, Connection};

use serde::Serialize;

use sha2::{Digest, Sha256};

use shakmaty::Position;

use std::{path::PathBuf, sync::Mutex, time::Instant};

use tauri::{Emitter, Manager};

struct App {

    review_ticket: std::sync::atomic::AtomicU64,

    game: Mutex<Game>,

    db: Mutex<Connection>,

    analyst: Mutex<Option<Engine>>,

    opponent: Mutex<Option<Engine>>,

    engine_path: PathBuf,

    hash: u32,

    threads: u32,

    language: Mutex<language::Runtime>,

}

fn save(a: &App, g: &Game) -> Result<(), String> {

    let mut db = a.db.lock().unwrap();

    let tx = db.transaction().map_err(|e| e.to_string())?;

    tx.execute("INSERT INTO games(id,name,data) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET name=excluded.name,data=excluded.data",params![g.id,g.name,serde_json::to_string(g).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;

    tx.execute(

        "INSERT OR REPLACE INTO settings(key,value) VALUES('active',?1)",

        [&g.id],

    )

    .map_err(|e| e.to_string())?;

    tx.commit().map_err(|e| e.to_string())

}

#[tauri::command]

fn snapshot(a: tauri::State<App>, node: Option<usize>) -> Result<Snapshot, String> {

    let mut g = a.game.lock().unwrap();

    let s = g.snapshot(node)?;

    save(&a, &g)?;

    Ok(s)

}

#[tauri::command]

async fn new_game(
    app: tauri::AppHandle,

    root: Option<String>,

    human: String,

    difficulty: u32,

    base_ms: f64,

    increment_ms: f64,

    chess960: bool,

    ai_opponent: Option<provider_play::Selection>,

) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
    let a = app.state::<App>();

    if let Some(ref selection) = ai_opponent { provider_play::validate_selection(selection)?; if human == "both" { return Err("Choose a color for provider play".into()); } }

    let mut old = a.game.lock().unwrap();

    old.pause();

    save(&a, &old)?;

    let mut g = Game::new(

        root.filter(|s| !s.is_empty()).unwrap_or(START.into()),

        human,

        difficulty,

        base_ms.max(0.),

        increment_ms.max(0.),

        chess960,

    )?;

    g.ai_opponent = ai_opponent;

    if let Some(ref selection) = g.ai_opponent { g.name = format!("vs {} / {}",selection.provider,selection.display_name); }

    if g.base_ms > 0. {

        g.tick = Some(Instant::now())

    }

    *old = g;

    save(&a, &old)?;

    old.snapshot(None)

    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]

fn commit_move(

    a: tauri::State<App>,

    game_id: String,

    revision: u64,

    uci: String,

    premove: bool,

) -> Result<Snapshot, String> {

    let mut g = a.game.lock().unwrap();

    if g.id != game_id {

        return Err("Stale game".into());

    }

    if let Err(e) = g.commit(&uci, revision, premove) {

        save(&a, &g)?;

        return Err(e);

    }

    save(&a, &g)?;

    g.snapshot(None)

}

#[tauri::command]

fn control(

    a: tauri::State<App>,

    action: String,

    game_id: Option<String>,

    revision: Option<u64>,

) -> Result<Snapshot, String> {

    let mut g = a.game.lock().unwrap();

    if game_id.as_ref().is_some_and(|id| id != &g.id) || revision.is_some_and(|r| r != g.revision) {

        return Err("Stale control request".into());

    }

    match action.as_str() {

        "undo" => g.undo(),

        "pause" => {

            g.pause();

            g.revision += 1

        }

        "resume" => {

            g.paused = false;

            g.revision += 1;

            if g.base_ms > 0. && g.result.is_none() {

                g.tick = Some(Instant::now())

            }

        }

        "resign" => g.resign(),

        "draw" => {

            g.pause();

            g.result = Some("Draw by agreement".into());

            g.revision += 1

        }

        _ => return Err("Unknown action".into()),

    }

    save(&a, &g)?;

    g.snapshot(None)

}

fn run_engine(

    a: &App,

    g: &Game,

    node: usize,

    opponent: bool,

    budget: u32,

    multipv: usize,

    restricted: Option<&str>,

) -> Result<Search, String> {

    let lock = if opponent { &a.opponent } else { &a.analyst };

    let mut guard = lock.lock().unwrap();

    {

        let current = a.game.lock().unwrap();

        if current.id != g.id || current.revision != g.revision {

            return Err("Stale engine work cancelled".into());

        }

    }

    if guard.is_none() {

        *guard = Some(Engine::new(

            &a.engine_path,

            if opponent { 2 } else { a.threads },

            if opponent { 64 } else { a.hash },

        )?)

    }

    let result = guard.as_mut().unwrap().search(

        &g.engine_position(node),

        budget,

        multipv,

        restricted,

        if opponent && g.difficulty >= 1320 && g.difficulty < 3200 {

            Some(g.difficulty)

        } else {

            None

        },

        g.chess960,

    );

    if result.is_err() {

        *guard = None;

    }

    let mut r = result?;

    let p = game::position(&g.nodes[node].fen, g.chess960)?;

    for l in &mut r.lines {

        l.san = g.legal_line(node, &l.pv)?;

        if p.turn() == shakmaty::Color::Black {

            l.cp = l.cp.map(|v| -v);

            l.mate = l.mate.map(|v| -v);

            if let Some(w) = l.wdl {

                l.wdl = Some([w[2], w[1], w[0]])

            }

        }

    }

    Ok(r)

}

#[derive(Serialize)]

struct Analysis {

    game_id: String,

    revision: u64,

    node: usize,

    sequence: u64,

    search: Search,

    label: Option<String>,

    loss: Option<f64>,

    best_alternative: Option<String>,

    explanation: String,

    detail: String,

    model: String,

}

fn compare_decision(

    a: &App,

    g: &Game,

    node: usize,

    fast: bool,

    child: &Search,

) -> Result<(Option<String>, Option<f64>, Option<String>), String> {

    if let Some(cached)=grading::cached(a,g,node){return Ok(cached.decision);}

    let label = None;

    let loss = None;

    let best_alternative = None;

    if let Some(parent) = g.nodes[node].parent {

        let cached = if fast {

            g.nodes[parent].evidence.as_ref()

                .and_then(|v| serde_json::from_value::<Search>(v["search"].clone()).ok())

                .filter(|s| s.engine.contains("Stockfish 19") && s.lines.len() >= 3 && s.lines.iter().all(|l|

                    !l.bound && l.depth >= 8 && g.legal_line(parent, &l.pv).is_ok()))

        } else { None };

        let best = match cached {

            Some(s) => s,

            None => run_engine(a, g, parent, false, if fast { 90 } else { 350 }, 3, None)?,

        };

        let played = if let Some(candidate) = same_root_candidate(g,node,&best) {

            candidate

        } else if let Some(child_root) = played_from_child(g,node,child) {

            child_root

        } else { run_engine(

            a,

            g,

            parent,

            false,

            if fast { 90 } else { 350 },

            1,

            Some(&g.nodes[node].uci),

        )? };

        return decision_from_search(a, g, node, &best, &played, if fast { 60 } else { 1100 });

    }

    Ok((label, loss, best_alternative))

}

// Prefer comparable candidates from the same root. Stockfish normalizes scores

// using root material, so a legal child continuation alone does not make a

// capture/promotion score numerically interchangeable with its parent score.

pub(crate) fn same_root_candidate(g:&Game,node:usize,best:&Search)->Option<Search> {

    let parent=g.nodes.get(node)?.parent?;

    let top=best.lines.first()?;

    if top.bound || top.depth<8 || top.pv.first()!=Some(&best.best) {return None;}

    let played=&g.nodes[node].uci;

    let line=best.lines.iter().find(|l| !l.bound && l.depth>=8

        && l.depth.abs_diff(top.depth)<=1 && l.pv.first()==Some(played)

        && g.legal_line(parent,&l.pv).is_ok())?;

    Some(Search {lines:vec![line.clone()],best:played.clone(),elapsed_ms:0.,engine:best.engine.clone()})

}

// Quiet moves retain root material. Material-changing moves require a same-root

// candidate or a restricted parent search; displayed child evaluation stays raw.

fn played_from_child(g:&Game,node:usize,child:&Search)->Option<Search> {

    let parent=g.nodes.get(node)?.parent?;

    let before=game::position(&g.nodes[parent].fen,g.chess960).ok()?;

    let moved=g.nodes[node].uci.parse::<shakmaty::uci::UciMove>().ok()?.to_move(&before).ok()?;

    if moved.is_capture() || matches!(moved,shakmaty::Move::Normal{promotion:Some(_),..}) {return None;}

    let mut line=child.lines.first()?.clone();

    if line.bound || line.depth<8 || line.pv.is_empty(){return None;}

    line.pv.insert(0,g.nodes[node].uci.clone());

    line.san=g.legal_line(parent,&line.pv).ok()?;

    line.depth+=1;

    line.multipv=1;

    Some(Search {lines:vec![line],best:g.nodes[node].uci.clone(),elapsed_ms:0.,engine:child.engine.clone()})

}

fn decision_from_search(

    a:&App,g:&Game,node:usize,best:&Search,played:&Search,refinement_ms:u32,

) -> Result<(Option<String>,Option<f64>,Option<String>),String> {

    if let Some(cached)=grading::cached(a,g,node){return Ok(cached.decision);}

    if refinement_ms>=300 {return Ok(grading::resolve(a,g,node,best,played)?.decision);}

    decision_from_search_uncached(a,g,node,best,played,refinement_ms)

}

fn decision_from_search_uncached(

    a: &App,

    g: &Game,

    node: usize,

    best: &Search,

    played: &Search,

    refinement_ms: u32,

) -> Result<(Option<String>, Option<f64>, Option<String>), String> {

    let (mut label, mut loss, mut best_alternative) = (None, None, None);

    let Some(parent) = g.nodes[node].parent else {

        return Ok((label, loss, best_alternative));

    };

    if let (Some(b), Some(p)) = (best.lines.first(), played.lines.first()) {

        if !b.bound && !p.bound && b.pv.first()==Some(&best.best)

            && p.pv.first()==Some(&g.nodes[node].uci) {

                let white = game::position(&g.nodes[parent].fen, g.chess960)?.turn()

                    == shakmaty::Color::White;

            if let (Some(bp), Some(pp)) = (feedback_model::expected(b, white), feedback_model::expected(p, white)) {

                let v = (bp - pp).clamp(0., 1.);

                loss = Some(v);

                let name = if best.best == g.nodes[node].uci || feedback_model::equal_score(b,p) {

                    "Best"

                } else {

                    feedback_model::grade(v)

                };

                label = Some(

                    if classification::book(g, node) && v < 0.05 {

                            "Book"

                        } else {

                            classification::refine(a, g, node, &best, &played, v, refinement_ms)?.unwrap_or(name)

                        }

                        .into(),

                );

                best_alternative = b.san.first().cloned();

            }

        }

    }

    Ok((label, loss, best_alternative))

}



fn analyze_impl(

    app: &tauri::AppHandle,

    a: &App,

    game_id: String,

    revision: u64,

    node: usize,

    sequence: u64,

    budget: u32,

    multipv: usize,

    fast: bool,

) -> Result<Analysis, String> {

    let g = a.game.lock().unwrap().clone();

    if g.id != game_id || g.revision != revision || node >= g.nodes.len() {

        return Err("Stale analysis request".into());

    }

    let search = run_engine(

        &a,

        &g,

        node,

        false,

        if fast {

            budget.clamp(50, 100)

        } else {

            budget.clamp(150, 5000)

        },

        multipv,

        None,

    )?;

    let _=app.emit("quick-analysis",serde_json::json!({"game_id":game_id,"revision":revision,"node":node,"sequence":sequence,"search":search,"label":null,"loss":null,"best_alternative":null,"explanation":"Checking the decision with comparable root searches…","model":"raw-stockfish-provisional"}));

    {

        let current = a.game.lock().unwrap();

        if current.id != g.id || current.revision != g.revision {

            return Err("Stale analysis work cancelled".into());

        }

    }

    let (label, loss, best_alternative) = compare_decision(a, &g, node, fast, &search)?;

    let facts = coaching::consequence(&g, node)?

        .into_iter()

        .chain(coaching::line_facts(&g, node, &search)?)

        .collect::<Vec<_>>();

    let explanation = coach_answers::friendly(

        &g,

        node,

        label.as_deref(),

        best_alternative.as_deref(),

        &search,

        &coach_answers::language(&a),

    )?;

    let detail = if matches!(

        label.as_deref(),

        Some("Inaccuracy" | "Mistake" | "Blunder" | "Miss")

    ) {

        coach_answers::mistake_reason(

            &g,

            node,

            best_alternative.as_deref(),

            &search,

            &coach_answers::language(&a),

        )?

    } else {

        facts.join(" ")

    };

    let mut current = a.game.lock().unwrap();

    if current.id != game_id || current.revision != revision {

        return Err("Stale analysis discarded".into());

    }

    current.nodes[node].evidence = Some(

        serde_json::json!({"search":search,"label":label,"loss":loss,"best_alternative":best_alternative,"explanation":explanation,"detail":detail,"facts":facts}),

    );

    save(&a, &current)?;

    // Historical evidence is immutable per node. The frontend merges it into its

    // cache, and guards position-specific presentation by revision and sequence.

    Ok(Analysis {

        game_id,

        revision,

        node,

        sequence,

        search,

        label,

        loss,

        best_alternative,

        explanation,

        detail,

        model: format!("{}; raw Stockfish evaluation; provisional feedback", feedback_model::ID),

    })

}

#[tauri::command]

async fn analyze(

    app: tauri::AppHandle,

    game_id: String,

    revision: u64,

    node: usize,

    sequence: u64,

    budget: u32,

    multipv: usize,

    fast: Option<bool>,

) -> Result<Analysis, String> {

    tauri::async_runtime::spawn_blocking(move || {

        let a = app.state::<App>();

        analyze_impl(

            &app,

            &a,

            game_id,

            revision,

            node,

            sequence,

            budget,

            multipv,

            fast.unwrap_or(false),

        )

    })

    .await

    .map_err(|e| e.to_string())?

}

#[tauri::command]

async fn warm_engines(app: tauri::AppHandle) -> Result<(), String> {

    tauri::async_runtime::spawn_blocking(move || {

        let a = app.state::<App>();

        for (lock, threads, hash) in [(&a.analyst, a.threads, a.hash), (&a.opponent, 2, 64)] {

            let mut guard = lock.lock().map_err(|e| e.to_string())?;

            if guard.is_none() {

                *guard = Some(Engine::new(&a.engine_path, threads, hash)?);

            }

        }

        Ok(())

    })

    .await

    .map_err(|e| e.to_string())?

}

#[derive(Serialize)]

struct Opponent {

    game_id: String,

    revision: u64,

    uci: String,

    elapsed_ms: f64,

    policy: String,

}

#[tauri::command]

async fn opponent(

    a: tauri::State<'_, App>,

    game_id: String,

    revision: u64,

) -> Result<Opponent, String> {

    let g = a.game.lock().unwrap().clone();

    if g.id != game_id || g.revision != revision || g.result.is_some() || g.paused {

        return Err("Stale opponent request".into());

    }

    let low = g.difficulty < 1320;

    let r = run_engine(

        &a,

        &g,

        g.live,

        true,

        if low { 400 } else { 650 },

        if low { 16 } else { 1 },

        None,

    )?;

    let chosen = if low {

        let opening = classification::opening_candidates(&g, &r);

        let count = r.lines.len();

        let index = if count == 0 {

            0

        } else {

            let seed = g.id.bytes().map(|v| v as usize).sum::<usize>() + g.live * 17;

            let width = if g.difficulty <= 400 {

                12

            } else if g.difficulty <= 900 {

                7

            } else {

                3

            };

            seed % count.min(width)

        };

        if !opening.is_empty() {

            let seed = g.id.bytes().map(|v| v as usize).sum::<usize>() + g.live * 17;

            opening[seed % opening.len()].clone()

        } else {

            r.lines

                .get(index)

                .and_then(|l| l.pv.first())

                .cloned()

                .unwrap_or(r.best)

        }

    } else {

        r.best

    };

    let now = a.game.lock().unwrap();

    if now.id != game_id || now.revision != revision {

        return Err("Stale opponent result discarded".into());

    }

    Ok(Opponent {

        game_id,

        revision,

        uci: chosen,

        elapsed_ms: r.elapsed_ms,

        policy: if low {

            "Uncalibrated local candidate sampling with evidence-checked opening preferences; numeric level is an estimate"

        } else {

            "Stockfish supported Elo / maximum"

        }

        .into(),

    })

}

#[derive(Serialize)]

struct Saved {

    id: String,

    name: String,

    result: Option<String>,

    moves: usize,

}

#[tauri::command]

fn library(a: tauri::State<App>) -> Result<Vec<Saved>, String> {

    let db = a.db.lock().unwrap();

    let mut s = db

        .prepare("SELECT data FROM games ORDER BY rowid DESC")

        .map_err(|e| e.to_string())?;

    let rows = s

        .query_map([], |r| r.get::<_, String>(0))

        .map_err(|e| e.to_string())?;

    Ok(rows

        .filter_map(|r| serde_json::from_str::<Game>(&r.ok()?).ok())

        .map(|g| Saved {

            id: g.id,

            name: g.name,

            result: g.result,

            moves: g.nodes.len() - 1,

        })

        .collect())

}

#[tauri::command]

fn load_game(a: tauri::State<App>, id: String) -> Result<Snapshot, String> {

    let data: String =

        a.db.lock()

            .unwrap()

            .query_row("SELECT data FROM games WHERE id=?1", [id], |r| r.get(0))

            .map_err(|e| e.to_string())?;

    let mut g = a.game.lock().unwrap();

    g.pause();

    save(&a, &g)?;

    *g = serde_json::from_str(&data).map_err(|e| e.to_string())?;

    g.revision += 1;

    save(&a, &g)?;

    g.snapshot(None)

}

#[tauri::command]

fn preferences(a: tauri::State<App>, value: Option<String>) -> Result<String, String> {

    let db = a.db.lock().unwrap();

    if let Some(v) = value {

        let _: serde_json::Value = serde_json::from_str(&v).map_err(|e| e.to_string())?;

        db.execute(

            "INSERT OR REPLACE INTO settings(key,value) VALUES('preferences',?1)",

            [v],

        )

        .map_err(|e| e.to_string())?;

    }

    Ok(db

        .query_row(

            "SELECT value FROM settings WHERE key='preferences'",

            [],

            |r| r.get(0),

        )

        .unwrap_or("{}".into()))

}

#[tauri::command]

fn export_game(a: tauri::State<App>, format: String) -> Result<String, String> {

    let g = a.game.lock().unwrap();

    if format == "json" {

        return serde_json::to_string_pretty(&*g).map_err(|e| e.to_string());

    }

    Ok(pgn::export(&g))

}

#[tauri::command]

fn import_backup(a: tauri::State<App>, data: String) -> Result<Snapshot, String> {

    let mut next: Game = serde_json::from_str(&data).map_err(|e| e.to_string())?;

    next.validate_backup()?;

    next.id = uuid::Uuid::new_v4().to_string();

    next.revision += 1;

    let mut g = a.game.lock().unwrap();

    g.pause();

    save(&a, &g)?;

    *g = next;

    save(&a, &g)?;

    g.snapshot(None)

}

#[tauri::command]

fn diagnostics(a: tauri::State<App>) -> serde_json::Value {

    let mut s = sysinfo::System::new_all();

    s.refresh_all();

    serde_json::json!({"engine":"Stockfish 19","feedback_model":feedback_model::ID,"database_path":a.db.lock().unwrap().path(),"analyst_threads":a.threads,"hash_mb":a.hash,"opponent_threads":2,"available_memory":s.available_memory(),"total_memory":s.total_memory(),"cpu":s.cpus().first().map(|c|c.brand()),"process_memory":s.process(sysinfo::get_current_pid().unwrap()).map(|p|p.memory()),"note":"Native process only; WebView child memory requires external process measurement"})

}

#[tauri::command]

fn preview_position() -> Result<Snapshot, String> {

    Game::new(START.into(), "both".into(), 400, 0., 0., false)?.snapshot(None)

}

#[tauri::command]

fn finish_background_qa(app:tauri::AppHandle)->Result<(),String> {

    if std::env::var_os("LOCAL_CHESS_COACH_BACKGROUND_QA").is_none()

        || !std::env::var("LOCAL_CHESS_COACH_DATA_DIR").unwrap_or_default().contains("installed-profile-024") {

        return Err("Available only in the isolated background QA profile".into());

    }

    app.exit(0);Ok(())

}

fn main() {

    let mut context=tauri::generate_context!();

    if std::env::var_os("LOCAL_CHESS_COACH_BACKGROUND_QA").is_some(){for window in &mut context.config_mut().app.windows{window.visible=false;}}

    #[cfg(debug_assertions)]

    if std::env::args().any(|a| a == "--test-driver") {

        driver::run();

        return;

    }

    tauri::Builder::default().plugin(tauri_plugin_dialog::init()).setup(|app|{let data=std::env::var_os("LOCAL_CHESS_COACH_DATA_DIR").map(PathBuf::from).unwrap_or(app.path().app_data_dir()?);std::fs::create_dir_all(&data)?;let db=Connection::open(data.join("coach.sqlite"))?;db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS games(id TEXT PRIMARY KEY,name TEXT NOT NULL,data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);")?;let active:Option<String>=db.query_row("SELECT g.data FROM games g JOIN settings s ON s.key='active' AND s.value=g.id",[],|r|r.get(0)).ok();let mut game=active.and_then(|s|serde_json::from_str::<Game>(&s).ok()).unwrap_or(Game::new(START.into(),"w".into(),400,0.,0.,false).unwrap());if game.ai_opponent.is_some(){game.pause();}let resource=app.path().resource_dir()?.join("resources/stockfish-windows-x86-64-universal.exe");let path=if resource.exists(){resource}else{PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/stockfish-windows-x86-64-universal.exe")};let bytes=std::fs::read(&path)?;let hash=format!("{:x}",Sha256::digest(bytes));if hash!="45bc8e4969147db9c2eb533810637994619bff0eacc81ccfd9854394901bcbd0"{return Err("Engine checksum mismatch".into())}let s=sysinfo::System::new_all();let low=s.available_memory()<3*1024*1024*1024;if std::env::var_os("LOCAL_CHESS_COACH_BACKGROUND_QA").is_some(){if let Some(w)=app.get_webview_window("main"){w.hide()?;}}app.manage(App{review_ticket:std::sync::atomic::AtomicU64::new(0),game:Mutex::new(game),db:Mutex::new(db),analyst:Mutex::new(None),opponent:Mutex::new(None),engine_path:path,hash:if low{64}else{256},threads:if low{2}else{6},language:Mutex::new(language::Runtime::default())});Ok(())}).invoke_handler(tauri::generate_handler![provider_play::provider_discover,provider_play::provider_opponent,provider_play::provider_cancel,provider_play::provider_login,provider_play::provider_install,provider_play::provider_source,provider_play::provider_diagnostics,provider_play::provider_local,finish_background_qa,reference_engine::evaluate_reference,learning::learning,coach_voice::coach_messages,coach_voice::coach_reply,preview_position,warm_engines,snapshot,new_game,commit_move,control,analyze,opponent,library,load_game,preferences,export_game,import_backup,diagnostics,workflows::import_pgn,workflows::study,workflows::branch_move,workflows::annotate_node,workflows::rename_game,workflows::training_progress,workflows::record_metrics,workflows::benchmark,workflows::review_game,workflows::review_report,workflows::cancel_review,workflows::review_line,workflows::review_variation,workflows::save_export,local_ai::warm_local_ai,language::provider_config,language::provider_key,language::provider_test,language::narrate,speech::speak_local,neural_voice::neural_speech,neural_voice::cancel_neural]).on_window_event(|w,e|{if matches!(e,tauri::WindowEvent::CloseRequested{..}){provider_play::stop();let a=w.state::<App>();let mut g=a.game.lock().unwrap();g.pause();let _=save(&a,&g);}}).build(context).expect("Application initialization failed").run(|app,event|{if matches!(event,tauri::RunEvent::Exit){provider_play::stop();speech::stop();local_ai::stop();reference_engine::stop();let a=app.state::<App>();*a.analyst.lock().unwrap()=None;*a.opponent.lock().unwrap()=None;}});

}



#[cfg(test)]

mod comparison_evidence_tests {

    use super::*;

    fn played(root:&str,uci:&str)->Game {

        let mut g=Game::new(root.into(),"both".into(),400,0.,0.,false).unwrap();

        g.commit(uci,0,false).unwrap();g

    }

    fn result(uci:&str,cp:i32,depth:u32)->Search {

        Search{lines:vec![engine::Line{depth,cp:Some(cp),pv:vec![uci.into()],..Default::default()}],best:uci.into(),elapsed_ms:60.,engine:"Stockfish 19".into()}

    }

    #[test]

    fn material_changes_reject_child_root_score_reuse() {

        for (root,moved,reply) in [

            ("4k3/8/8/8/3p4/4P3/8/4K3 w - - 0 1","e3d4","e8f8"),

            ("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1","e5d6","e8f8"),

            ("4k3/P7/8/8/8/8/8/4K3 w - - 0 1","a7a8q","e8f7"),

            ("1r2k3/P7/8/8/8/8/8/4K3 w - - 0 1","a7b8q","e8f7"),

        ] {

            let g=played(root,moved);let child=result(reply,170,14);

            assert!(g.legal_line(1,&child.lines[0].pv).is_ok());

            assert!(played_from_child(&g,1,&child).is_none(),"{moved} changes root normalization");

        }

    }

    #[test]

    fn quiet_moves_and_castling_preserve_raw_child_scores() {

        for (root,moved,reply) in [

            (START,"e2e4","e7e5"),

            ("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1","e1g1","e8c8"),

        ] {

            let g=played(root,moved);let child=result(reply,64,14);

            let reused=played_from_child(&g,1,&child).unwrap();

            assert_eq!(reused.lines[0].cp,child.lines[0].cp);

            assert_eq!(reused.lines[0].pv,vec![moved.to_owned(),reply.to_owned()]);

        }

    }

    #[test]

    fn same_root_capture_candidate_is_preferred_and_keeps_its_score() {

        let g=played("4k3/8/8/8/3p4/4P3/8/4K3 w - - 0 1","e3d4");

        let mut best=result("e1f1",85,15);

        best.lines.push(engine::Line{depth:15,cp:Some(64),pv:vec!["e3d4".into(),"e8f8".into()],..Default::default()});

        let candidate=same_root_candidate(&g,1,&best).unwrap();

        assert_eq!(candidate.lines[0].cp,Some(64));

        best.lines[1].depth=12;

        assert!(same_root_candidate(&g,1,&best).is_none(),"incomparable stale depth");

        best.lines[1].depth=15;best.lines[1].bound=true;

        assert!(same_root_candidate(&g,1,&best).is_none());

        best.lines[1].bound=false;best.lines[1].pv=vec!["e3d4".into(),"e8e1".into()];

        assert!(same_root_candidate(&g,1,&best).is_none(),"illegal continuation");

        best.best="e3d4".into();

        assert!(same_root_candidate(&g,1,&best).is_none(),"top score must belong to actual bestmove");

    }

}

