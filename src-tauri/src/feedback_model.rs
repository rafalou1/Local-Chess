//! Local human-oriented feedback; deliberately separate from Stockfish's raw
//! evaluation and perfect-play WDL. This is not Chess.com's proprietary model.
use crate::engine::Line;

pub const ID: &str = "human-feedback-v1";

/// Equal exact scores at comparable depth are equally best choices. Do not use
/// saturated winning chances (or an epsilon) to make inferior moves 'Best'.
pub fn equal_score(a: &Line, b: &Line) -> bool {
    if a.bound || b.bound || a.depth < 8 || b.depth < 8 || a.depth.abs_diff(b.depth)>1 {return false;}
    match (a.mate,b.mate) {
        (Some(x),Some(y)) => x!=0 && x==y,
        (None,None) => a.cp.is_some() && a.cp==b.cp,
        _ => false,
    }
}

pub fn expected(line: &Line, white: bool) -> Option<f64> {
    if line.bound { return None; }
    let score = if let Some(mate) = line.mate {
        if mate == 0 { return None; }
        if mate > 0 { 1. } else { 0. }
    } else {
        let pawns = line.cp? as f64 / 100.;
        // Gentle near equality, increasingly decisive for substantial material
        // advantages. No opponent difficulty or position-specific exceptions.
        0.5 + 0.5 * (0.11 * pawns + 0.003 * pawns.powi(3)).tanh()
    };
    Some(if white { score } else { 1. - score })
}

pub fn grade(loss: f64) -> &'static str {
    if loss < 0.02 { "Excellent" }
    else if loss < 0.05 { "Good" }
    else if loss < 0.10 { "Inaccuracy" }
    else if loss < 0.20 { "Mistake" }
    else { "Blunder" }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(cp:i32)->Line { Line { cp:Some(cp), ..Default::default() } }
    #[test]
    fn best_ties_require_exact_comparable_scores() {
        let a=Line{cp:Some(-900),depth:18,..Default::default()};
        assert!(equal_score(&a,&a));
        assert!(!equal_score(&a,&Line{cp:Some(-899),..a.clone()}));
        assert!(!equal_score(&a,&Line{depth:15,..a.clone()}));
        assert!(!equal_score(&a,&Line{bound:true,..a.clone()}));
        assert!(!equal_score(&Line::default(),&Line::default()));
        let mate=Line{mate:Some(3),depth:18,..Default::default()};
        assert!(equal_score(&mate,&mate));
        assert!(!equal_score(&mate,&Line{mate:Some(4),..mate.clone()}));
        assert!(!equal_score(&mate,&a));
    }
    #[test]
    fn monotone_symmetric_and_saturated() {
        let mut previous=0.;
        for cp in -2000..=2000 {
            let w=expected(&line(cp),true).unwrap();
            assert!(w>=previous && (0. ..=1.).contains(&w));
            assert!((w+expected(&line(cp),false).unwrap()-1.).abs()<1e-12);
            assert!((w+expected(&line(-cp),true).unwrap()-1.).abs()<1e-12);
            previous=w;
        }
        assert!(expected(&line(900),true).unwrap()>0.998);
    }
    #[test]
    fn missing_bounds_and_mates_are_explicit() {
        assert_eq!(expected(&Line::default(),true),None);
        assert_eq!(expected(&Line {bound:true,cp:Some(50),..Default::default()},true),None);
        assert_eq!(expected(&Line {mate:Some(3),..Default::default()},true),Some(1.));
        assert_eq!(expected(&Line {mate:Some(-3),..Default::default()},true),Some(0.));
    }
    #[test]
    fn feedback_thresholds() {
        for (loss,name) in [(0.,"Excellent"),(0.02,"Good"),(0.05,"Inaccuracy"),(0.10,"Mistake"),(0.20,"Blunder")] {
            assert_eq!(grade(loss),name);
        }
        assert_eq!(grade(expected(&line(75),true).unwrap()-expected(&line(10),true).unwrap()),"Good");
        assert_eq!(grade(expected(&line(70),false).unwrap()-expected(&line(250),false).unwrap()),"Mistake");
    }
}
