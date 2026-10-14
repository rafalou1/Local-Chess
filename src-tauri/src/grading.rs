//! One final decision per legal history/configuration, shared by play, review
//! and hypothetical analysis. UI language and opponent strength never enter it.
use crate::{App,game::Game,engine::Search};
use serde::{Serialize,Deserialize};
use sha2::{Digest,Sha256};
use std::sync::{Mutex,OnceLock};
pub const POLICY:&str="grade-v14-reviewed-sacrifice";
pub type Decision=(Option<String>,Option<f64>,Option<String>);
#[derive(Clone,Serialize,Deserialize)]
pub struct Resolved {pub best:Search,pub played:Search,pub decision:Decision}
fn key(g:&Game,node:usize,threads:u32,hash:u32)->String {
    let history:Vec<_>=g.path(node).iter().map(|&n|(&g.nodes[n].fen,&g.nodes[n].uci)).collect();
    format!("{POLICY}:{:x}",Sha256::digest(serde_json::to_vec(&(g.chess960,history,threads,hash)).unwrap()))
}
fn valid(g:&Game,node:usize,r:&Resolved)->bool {
    let Some(parent)=g.nodes[node].parent else {return false;};
    let Some(top)=r.best.lines.first() else {return false;};
    let Some(played)=r.played.lines.first() else {return false;};
    r.decision.0.is_some() && r.decision.1.is_some_and(|loss|loss.is_finite() && (0. ..=1.).contains(&loss))
        && !top.bound && !played.bound && top.depth>=8 && played.depth>=8
        && top.pv.first()==Some(&r.best.best) && played.pv.first()==Some(&g.nodes[node].uci)
        && r.best.lines.iter().all(|l|g.legal_line(parent,&l.pv).is_ok()) && g.legal_line(parent,&played.pv).is_ok()
}
pub fn cached(a:&App,g:&Game,node:usize)->Option<Resolved> {
    let db=a.db.lock().ok()?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS grading_evidence(key TEXT PRIMARY KEY,data TEXT NOT NULL)").ok()?;
    let text:String=db.query_row("SELECT data FROM grading_evidence WHERE key=?1",[key(g,node,a.threads,a.hash)],|r|r.get(0)).ok()?;
    let resolved=serde_json::from_str(&text).ok()?;
    valid(g,node,&resolved).then_some(resolved)
}
pub fn resolve(a:&App,g:&Game,node:usize,best:&Search,_played:&Search)->Result<Resolved,String> {
    static LOCK:OnceLock<Mutex<()>>=OnceLock::new();
    let _guard=LOCK.get_or_init(||Mutex::new(())).lock().map_err(|e|e.to_string())?;
    if let Some(r)=cached(a,g,node){return Ok(r);}
    let parent=g.nodes[node].parent.ok_or("No decision at root")?;
    let distinct:std::collections::BTreeSet<_>=best.lines.iter().filter_map(|l|l.pv.first()).collect();
    use shakmaty::Position;
    let required=crate::game::position(&g.nodes[parent].fen,g.chess960)?.legal_moves().len().min(3);
    let strong=best.lines.first().is_some_and(|top|best.lines.len()>=required && top.depth>=16 && top.pv.first()==Some(&best.best)
        && distinct.len()==best.lines.len() && best.lines.iter().all(|l|!l.bound && l.depth+2>=top.depth && g.legal_line(parent,&l.pv).is_ok()));
    let best=if strong {best.clone()}else{crate::run_engine(a,g,parent,false,350,3,None)?};
    // Always compare parent-root evidence. A caller's quiet child estimate must
    // not make review and manual analysis grade the same move differently.
    let played=if let Some(line)=crate::same_root_candidate(g,node,&best){line}
        else{crate::run_engine(a,g,parent,false,350,1,Some(&g.nodes[node].uci))?};
    let mut verified=g.clone();
    // Opportunity labels must use a previous shared final grade, not whichever
    // provisional evidence the calling screen happened to attach to its node.
    verified.nodes[parent].evidence=cached(a,g,parent).and_then(|r|r.decision.1).map(|loss|serde_json::json!({"loss":loss}));
    let decision=crate::decision_from_search_uncached(a,&verified,node,&best,&played,650)?;
    let r=Resolved{best,played,decision};
    if valid(g,node,&r) {
        let db=a.db.lock().map_err(|e|e.to_string())?;
        db.execute("INSERT OR REPLACE INTO grading_evidence(key,data) VALUES(?1,?2)",rusqlite::params![key(g,node,a.threads,a.hash),serde_json::to_string(&r).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    }
    Ok(r)
}
#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn evidence_key_ignores_language_identity_and_opponent_strength_but_keeps_history_and_config(){
  let mut g=Game::new(crate::game::START.into(),"w".into(),400,0.,0.,false).unwrap();g.commit("e2e4",0,false).unwrap();
  let original=key(&g,1,6,256);let mut other=g.clone();other.id="different".into();other.human="b".into();other.difficulty=3200;
  assert_eq!(original,key(&other,1,6,256));assert_ne!(original,key(&g,1,2,64));
  other.nodes[0].evidence=Some(serde_json::json!({"loss":0.3}));assert_eq!(original,key(&other,1,6,256));
  other=g.clone();other.nodes[0].fen=other.nodes[0].fen.replace("0 1","2 1");assert_ne!(original,key(&other,1,6,256));
 }
}
