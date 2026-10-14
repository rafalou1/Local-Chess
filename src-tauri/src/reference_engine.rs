//! Optional reference-compatible display only. Never used for grades, advice or opposition.
use crate::{engine::{Engine,Line,Search},game,App};
use serde::Serialize;
use sha2::{Digest,Sha256};
use shakmaty::{Position,Color,uci::UciMove,san::SanPlus};
use std::sync::{Mutex,atomic::{AtomicU64,Ordering}};
use tauri::Manager;
const ID:&str="Stockfish 19-Lite";
const SHA:&str="aa4c2609edd3093a436f0f3c26e1caa3804d7eff9d50f602b766472e1c515dab";
static ENGINE:Mutex<Option<Engine>>=Mutex::new(None);
static TICKET:AtomicU64=AtomicU64::new(0);
#[derive(Serialize)]
pub struct ReferenceEvaluation {game_id:String,revision:u64,fen:String,search:Search}
fn valid_search(fen:&str,chess960:bool,mut search:Search)->Result<Search,String> {
    let start=game::position(fen,chess960)?;
    if !search.best.is_empty() && search.best!="(none)" {
        search.best.parse::<UciMove>().map_err(|e|e.to_string())?.to_move(&start).map_err(|e|e.to_string())?;
    }
    for line in &mut search.lines {
        let mut p=start.clone();line.san.clear();
        for text in &line.pv {
            let m=text.parse::<UciMove>().map_err(|e|e.to_string())?.to_move(&p).map_err(|e|e.to_string())?;
            line.san.push(SanPlus::from_move(p.clone(),m).to_string());p.play_unchecked(m);
        }
        if start.turn()==Color::Black {
            line.cp=line.cp.map(|cp|-cp);line.mate=line.mate.map(|mate|-mate);
            line.wdl=line.wdl.map(|w|[w[2],w[1],w[0]]);
        }
    }
    search.engine=ID.into();Ok(search)
}
fn current(a:&App,id:&str,revision:u64,ticket:u64)->Result<bool,String> {
    let g=a.game.lock().map_err(|_|"Game state unavailable")?;
    if g.id!=id || g.revision!=revision || TICKET.load(Ordering::SeqCst)!=ticket {return Err("Stale reference evaluation".into());}
    Ok(g.chess960)
}
#[tauri::command]
pub async fn evaluate_reference(app:tauri::AppHandle,game_id:String,revision:u64,fen:String,budget:u32)->Result<ReferenceEvaluation,String> {
    let ticket=TICKET.fetch_add(1,Ordering::SeqCst)+1;
    tauri::async_runtime::spawn_blocking(move|| {
        let a=app.state::<App>();let chess960=current(&a,&game_id,revision,ticket)?;
        let position=game::position(&fen,chess960)?;let canonical=game::fen(&position);
        let mut engine=ENGINE.lock().map_err(|_|"Reference engine unavailable")?;
        current(&a,&game_id,revision,ticket)?;
        let search=if position.legal_moves().is_empty() || position.is_insufficient_material() {
            Search {lines:vec![Line {cp:Some(if position.is_checkmate(){if position.turn()==Color::White{-100000}else{100000}}else{0}),mate:position.is_checkmate().then_some(0),..Default::default()}],best:String::new(),elapsed_ms:0.,engine:ID.into()}
        } else {
            if engine.is_none() {
                #[cfg(target_arch="x86_64")]
                if !std::is_x86_feature_detected!("avx2") {return Err("Reference display engine requires AVX2; full coaching remains available".into());}
                let installed=app.path().resource_dir().map_err(|e|e.to_string())?.join("resources/stockfish-lite/stockfish-19-lite-avx2.exe");
                let path=if installed.exists(){installed}else{std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/stockfish-lite/stockfish-19-lite-avx2.exe")};
                let hash=format!("{:x}",Sha256::digest(std::fs::read(&path).map_err(|e|e.to_string())?));
                if hash!=SHA {return Err("Reference engine checksum mismatch".into());}
                *engine=Some(Engine::new_expected(&path,1,64,ID)?);
            }
            match engine.as_mut().unwrap().search(&format!("position fen {canonical}"),budget.clamp(50,500),1,None,None,chess960) {
                Ok(search)=>valid_search(&canonical,chess960,search)?,
                Err(error)=>{*engine=None;return Err(error);}
            }
        };
        current(&a,&game_id,revision,ticket)?;
        Ok(ReferenceEvaluation{game_id,revision,fen,search})
    }).await.map_err(|e|e.to_string())?
}
pub fn stop(){TICKET.fetch_add(1,Ordering::SeqCst);if let Ok(mut engine)=ENGINE.lock(){*engine=None;}}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legal_reference_pv_sign_and_notation_are_verified() {
        let fen="rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
        let search=Search{lines:vec![Line{cp:Some(-25),wdl:Some([10.,900.,90.]),pv:vec!["e7e5".into(),"g1f3".into()],..Default::default()}],best:"e7e5".into(),elapsed_ms:50.,engine:ID.into()};
        let checked=valid_search(fen,false,search.clone()).unwrap();
        assert_eq!(checked.lines[0].cp,Some(25));assert_eq!(checked.lines[0].wdl,Some([90.,900.,10.]));
        assert_eq!(checked.lines[0].san,vec!["e5","Nf3"]);
        let mut illegal=search.clone();illegal.lines[0].pv[0]="e7e4".into();assert!(valid_search(fen,false,illegal).is_err());
        illegal=search;illegal.best="e7e4".into();assert!(valid_search(fen,false,illegal).is_err());
        assert!(game::position("not a fen\nquit",false).is_err());
    }
    #[test]
    fn native_reference_resource_checksum_and_strict_identity() {
        let path=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/stockfish-lite/stockfish-19-lite-avx2.exe");
        assert_eq!(format!("{:x}",Sha256::digest(std::fs::read(&path).unwrap())),SHA);
        let mut engine=Engine::new_expected(&path,1,64,ID).unwrap();
        let search=engine.search(&format!("position fen {}",game::START),50,1,None,None,false).unwrap();
        assert_eq!(search.engine,ID);assert!(!search.lines.is_empty());
        assert!(Engine::new(&path,1,64).is_err(),"Lite must not masquerade as pinned full19");
    }
}
