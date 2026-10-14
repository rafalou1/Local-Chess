//! Debug-only browser test transport. Never compiled into release binaries.
use crate::{
    game::{Game, START},
    run_engine, App,
};
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Mutex,
};
pub fn run() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE games(id TEXT PRIMARY KEY,name TEXT,data TEXT);CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT);").unwrap();
    let a = App {
        review_ticket: std::sync::atomic::AtomicU64::new(0),
        game: Mutex::new(Game::new(START.into(), "w".into(), 400, 0., 0., false).unwrap()),
        db: Mutex::new(db),
        analyst: Mutex::new(None),
        opponent: Mutex::new(None),
        engine_path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/stockfish-windows-x86-64-universal.exe"),
        threads: 2,
        hash: 64,
        language: Mutex::new(crate::language::Runtime::default()),
    };
    for line in io::stdin().lock().lines() {
        let r = line
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<Value>(&s).map_err(|e| e.to_string()))
            .and_then(|v| dispatch(&a, &v));
        let response = match r {
            Ok(v) => json!({"ok":v}),
            Err(e) => json!({"error":e}),
        };
        println!("{response}");
        let _ = io::stdout().flush();
    }
}
fn dispatch(a: &App, v: &Value) -> Result<Value, String> {
    let cmd = v["cmd"].as_str().ok_or("Missing command")?;
    let args = &v["args"];
    match cmd {
        "new_game" => {
            let g = Game::new(
                args["root"].as_str().unwrap_or(START).into(),
                args["human"].as_str().unwrap_or("w").into(),
                args["difficulty"].as_u64().unwrap_or(400) as u32,
                args["baseMs"].as_f64().unwrap_or(0.),
                args["incrementMs"].as_f64().unwrap_or(0.),
                args["chess960"].as_bool().unwrap_or(false),
            )?;
            *a.game.lock().unwrap() = g;
            Ok(serde_json::to_value(a.game.lock().unwrap().snapshot(None)?).unwrap())
        }
        "snapshot" => Ok(serde_json::to_value(
            a.game
                .lock()
                .unwrap()
                .snapshot(args["node"].as_u64().map(|n| n as usize))?,
        )
        .unwrap()),
        "commit_move" | "branch_move" => {
            let mut g = a.game.lock().unwrap();
            if cmd == "branch_move" {
                g.live = args["node"].as_u64().unwrap() as usize;
                g.result = None;
            }
            g.commit(
                args["uci"].as_str().ok_or("uci")?,
                args["revision"].as_u64().ok_or("revision")?,
                args["premove"].as_bool().unwrap_or(false),
            )?;
            Ok(serde_json::to_value(g.snapshot(None)?).unwrap())
        }
        "control" => {
            let mut g = a.game.lock().unwrap();
            match args["action"].as_str().unwrap_or("") {
                "undo" => g.undo(),
                "pause" => g.pause(),
                _ => {}
            }
            Ok(serde_json::to_value(g.snapshot(None)?).unwrap())
        }
        "preferences" => Ok(json!("{}")),
        "record_metrics" => Ok(Value::Null),
        "opponent" => {
            let g = a.game.lock().unwrap().clone();
            let r = run_engine(a, &g, g.live, true, 200, 1, None)?;
            Ok(json!({"game_id":g.id,"revision":g.revision,"uci":r.best}))
        }
        "analyze" => {
            let g = a.game.lock().unwrap().clone();
            if args["gameId"] != g.id || args["revision"].as_u64() != Some(g.revision) {
                return Err("Stale analysis request".into());
            }
            let node = args["node"].as_u64().unwrap_or(g.live as u64) as usize;
            let r = run_engine(a, &g, node, false, 150, 3, None)?;
            let (label, loss, best_alternative) = crate::compare_decision(a, &g, node, true, &r)?;
            let explanation = crate::coaching::friendly(
                &g,
                node,
                label.as_deref(),
                best_alternative.as_deref(),
                &r,
            )?;
            Ok(
                json!({"game_id":g.id,"revision":g.revision,"node":node,"sequence":args["sequence"],"search":r,"label":label,"loss":loss,"best_alternative":best_alternative,"explanation":explanation,"model":"test-driver; same native classification"}),
            )
        }
        "export_game" => {
            let g = a.game.lock().unwrap();
            Ok(json!(if args["format"] == "json" {
                serde_json::to_string(&*g).unwrap()
            } else {
                crate::pgn::export(&g)
            }))
        }
        _ => Ok(Value::Null),
    }
}
