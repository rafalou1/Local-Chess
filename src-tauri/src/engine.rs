use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
#[derive(Clone, Default, Serialize, Deserialize, Debug)]
pub struct Line {
    pub depth: u32,
    pub cp: Option<i32>,
    pub mate: Option<i32>,
    pub wdl: Option<[f64; 3]>,
    pub bound: bool,
    pub pv: Vec<String>,
    pub san: Vec<String>,
    pub nodes: u64,
    pub nps: u64,
    pub multipv: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Search {
    pub lines: Vec<Line>,
    pub best: String,
    pub elapsed_ms: f64,
    pub engine: String,
}
pub struct Engine {
    identity: String,
    child: Child,
    input: ChildStdin,
    rx: Receiver<String>,
    #[cfg(windows)]
    _job: std::os::windows::io::OwnedHandle,
}
impl Drop for Engine {
    fn drop(&mut self) {
        let _ = writeln!(self.input, "quit");
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Engine {
    pub fn new(path: &Path, threads: u32, hash: u32) -> Result<Self, String> {
        Self::new_expected(path, threads, hash, "Stockfish 19")
    }
    pub(crate) fn new_expected(path: &Path, threads: u32, hash: u32, expected: &str) -> Result<Self, String> {
        let mut cmd = Command::new(path);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        let input = child.stdin.take().ok_or("Engine stdin")?;
        let output = child.stdout.take().ok_or("Engine stdout")?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                if let Ok(s) = line {
                    if tx.send(s).is_err() {
                        break;
                    }
                } else {
                    break;
                }
            }
        });
        #[cfg(windows)]
        let job = supervise(&child)?;
        let mut e = Self {
            identity: expected.into(),
            child,
            input,
            rx,
            #[cfg(windows)]
            _job: job,
        };
        e.send("uci")?;
        let mut identity = false;
        loop {
            let s = e.read(10_000)?;
            if s == format!("id name {expected}") {
                identity = true
            }
            if s == "uciok" {
                break;
            }
        }
        if !identity {
            return Err(format!("Expected pinned {expected}"));
        }
        for s in [
            format!("setoption name Threads value {threads}"),
            format!("setoption name Hash value {hash}"),
            "setoption name UCI_ShowWDL value true".into(),
        ] {
            e.send(&s)?
        }
        e.send("isready")?;
        while e.read(10_000)? != "readyok" {}
        Ok(e)
    }
    fn send(&mut self, s: &str) -> Result<(), String> {
        writeln!(self.input, "{s}")
            .and_then(|_| self.input.flush())
            .map_err(|e| e.to_string())
    }
    fn read(&self, ms: u64) -> Result<String, String> {
        self.rx
            .recv_timeout(Duration::from_millis(ms))
            .map_err(|e| format!("Engine timeout/disconnected: {e}"))
    }
    pub fn search(
        &mut self,
        position: &str,
        ms: u32,
        multipv: usize,
        restricted: Option<&str>,
        elo: Option<u32>,
        c960: bool,
    ) -> Result<Search, String> {
        self.send(&format!("setoption name UCI_Chess960 value {c960}"))?;
        self.send(&format!(
            "setoption name UCI_LimitStrength value {}",
            elo.is_some()
        ))?;
        if let Some(elo) = elo {
            self.send(&format!(
                "setoption name UCI_Elo value {}",
                elo.clamp(1320, 3190)
            ))?
        }
        self.send(&format!(
            "setoption name MultiPV value {}",
            multipv.clamp(1, 64)
        ))?;
        self.send(position)?;
        let started = Instant::now();
        self.send(&format!(
            "go movetime {}{}",
            ms.clamp(50, 10000),
            restricted
                .map(|m| format!(" searchmoves {m}"))
                .unwrap_or_default()
        ))?;
        let mut candidates = Candidates::default();
        loop {
            let s = self.read(ms as u64 + 5000)?;
            if let Some(rest) = s.strip_prefix("bestmove ") {
                let best=rest.split_whitespace().next().unwrap_or("").to_owned();
                return Ok(Search {
                    lines:candidates.finish(&best),
                    best,
                    elapsed_ms: started.elapsed().as_secs_f64() * 1000.,
                    engine: self.identity.clone(),
                });
            }
            if let Some(l) = parse(&s) {
                candidates.push(l,multipv.clamp(1,64));
            }
        }
    }
}
/// UCI ranks can change mid-iteration. Keep complete distinct snapshots rather
/// than combining new rank1 with an old rank2 which now describes the same move.
#[derive(Default)]
struct Candidates {
    depths:std::collections::BTreeMap<u32,Vec<Option<Line>>>,
    completed:std::collections::BTreeMap<u32,Vec<Line>>,
    moves:std::collections::BTreeMap<String,Line>,
}
impl Candidates {
    fn push(&mut self,line:Line,count:usize) {
        if line.bound || line.pv.is_empty() || line.multipv==0 || line.multipv>count {return;}
        let first=line.pv[0].clone();
        if self.moves.get(&first).is_none_or(|old|line.depth>=old.depth) {self.moves.insert(first,line.clone());}
        let slots=self.depths.entry(line.depth).or_insert_with(||vec![None;count]);
        slots[line.multipv-1]=Some(line.clone());
        if slots.iter().all(Option::is_some) {
            let rows:Vec<Line>=slots.iter().filter_map(Clone::clone).collect();
            let distinct:std::collections::BTreeSet<_>=rows.iter().map(|l|&l.pv[0]).collect();
            if distinct.len()==count {self.completed.insert(line.depth,rows);}
        }
    }
    fn finish(&self,best:&str)->Vec<Line> {
        if let Some(rows)=self.completed.values().rev().find(|rows|rows[0].pv[0]==best) {return rows.clone();}
        let Some(top)=self.moves.get(best) else {return vec![];};
        let mut rows=vec![top.clone()];
        if let Some(previous)=self.completed.values().next_back() {
            rows.extend(previous.iter().filter(|l|l.pv[0]!=best).cloned());
        }
        // No complete iteration (e.g. fewer legal moves than requested MultiPV):
        // distinct exact moves are usable, while grading still checks their depth.
        if rows.len()==1 {rows.extend(self.moves.values().filter(|l|l.pv[0]!=best).cloned());}
        for (index,line) in rows.iter_mut().enumerate(){line.multipv=index+1;}
        rows
    }
}
// An incomplete aspiration-bound update must not discard the last complete score.
fn retain_exact(previous: &mut Line, next: Line) {
    if !next.bound || previous.bound || previous.depth == 0 {
        *previous = next
    }
}
fn parse(s: &str) -> Option<Line> {
    if !s.starts_with("info depth ") || !s.contains(" pv ") {
        return None;
    }
    let t: Vec<&str> = s.split_whitespace().collect();
    let mut l = Line {
        multipv: 1,
        ..Default::default()
    };
    for i in 0..t.len() {
        let v = t.get(i + 1).copied().unwrap_or("");
        match t[i] {
            "depth" => l.depth = v.parse().ok()?,
            "multipv" => l.multipv = v.parse().ok()?,
            "cp" => l.cp = v.parse().ok(),
            "mate" => l.mate = v.parse().ok(),
            "nodes" => l.nodes = v.parse().unwrap_or(0),
            "nps" => l.nps = v.parse().unwrap_or(0),
            "lowerbound" | "upperbound" => l.bound = true,
            "wdl" => {
                l.wdl = Some([
                    v.parse().ok()?,
                    t.get(i + 2)?.parse().ok()?,
                    t.get(i + 3)?.parse().ok()?,
                ]);
            }
            "pv" => {
                l.pv = t[i + 1..].iter().map(|s| s.to_string()).collect();
                break;
            }
            _ => {}
        }
    }
    Some(l)
}
#[cfg(windows)]
pub(crate) fn supervise(child: &Child) -> Result<std::os::windows::io::OwnedHandle, String> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::JobObjects::*;
    unsafe {
        let raw = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if raw.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let job = OwnedHandle::from_raw_handle(raw);
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            raw,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of_val(&info) as u32,
        ) == 0
            || AssignProcessToJobObject(raw, child.as_raw_handle()) == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(job)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn candidate(depth:u32,rank:usize,m:&str,cp:i32)->Line {Line{depth,multipv:rank,pv:vec![m.into()],cp:Some(cp),..Default::default()}}
    #[test]
    fn interrupted_rank_change_never_duplicates_candidates_or_mixes_scores(){
        let mut c=Candidates::default();
        for (rank,m,cp) in [(1,"e2e4",50),(2,"d2d4",40),(3,"g1f3",30)] {c.push(candidate(16,rank,m,cp),3);}
        c.push(candidate(17,1,"d2d4",60),3);
        let rows=c.finish("d2d4");
        assert_eq!(rows[0].depth,17);assert_eq!(rows[0].cp,Some(60));
        assert_eq!(rows.iter().map(|l|&l.pv[0]).collect::<std::collections::BTreeSet<_>>().len(),3);
        c.push(Line{bound:true,..candidate(18,1,"d2d4",90)},3);
        assert_eq!(c.finish("d2d4")[0].cp,Some(60));
    }
    #[test]
    fn incomplete_new_iteration_preserves_complete_matching_best(){
        let mut c=Candidates::default();
        for (rank,m) in [(1,"e2e4"),(2,"d2d4"),(3,"g1f3")] {c.push(candidate(16,rank,m,50),3);}
        c.push(candidate(17,1,"e2e4",80),3);
        assert!(c.finish("e2e4").iter().all(|l|l.depth==16));
        assert!(c.finish("a2a4").is_empty());
    }
    #[test]
    fn incomplete_bound_keeps_last_exact_score() {
        let mut previous = Line {
            depth: 16,
            cp: Some(30),
            wdl: Some([80., 900., 20.]),
            ..Default::default()
        };
        retain_exact(
            &mut previous,
            Line {
                depth: 17,
                bound: true,
                cp: Some(100),
                ..Default::default()
            },
        );
        assert_eq!(previous.depth, 16);
        assert!(!previous.bound);
        retain_exact(
            &mut previous,
            Line {
                depth: 17,
                cp: Some(40),
                ..Default::default()
            },
        );
        assert_eq!(previous.cp, Some(40));
    }
    #[test]
    fn parse_bound_wdl() {
        let l=parse("info depth 18 multipv 2 score cp -31 lowerbound wdl 100 700 200 nodes 500 nps 1000 pv e2e4 e7e5").unwrap();
        assert_eq!(l.cp, Some(-31));
        assert!(l.bound);
        assert_eq!(l.wdl, Some([100., 700., 200.]));
        assert_eq!(l.pv.len(), 2);
    }
}
