import type {Snapshot} from './types';
export function ReviewMaterial({snap,color,language='en'}:{snap:Snapshot;color:string;language?:string}){
 const counts:Record<string,number>={};const values:Record<string,number>={p:1,n:3,b:3,r:5,q:9,k:0};const tally=(fen:string,side:string)=>{const result:Record<string,number>={};for(const c of fen.split(' ')[0])if(/[a-z]/i.test(c)&&(side==='w'?c===c.toUpperCase():c===c.toLowerCase()))result[c.toLowerCase()]=(result[c.toLowerCase()]||0)+1;return result};
 const enemy=color==='w'?'b':'w';let node=snap.node;while(snap.game.nodes[node]?.parent!==null&&snap.game.nodes[node]){const n=snap.game.nodes[node],parent=snap.game.nodes[n.parent!];if(parent.fen.split(' ')[1]===color){const before=tally(parent.fen,enemy),after=tally(n.fen,enemy);for(const role of Object.keys(before)){const removed=(before[role]||0)-(after[role]||0);if(removed>0)counts[role]=(counts[role]||0)+removed;}}node=n.parent!;}
 const material=(side:string)=>Object.entries(tally(snap.fen,side)).reduce((v,[r,n])=>v+(values[r]||0)*n,0),difference=material(color)-material(enemy);
 const roles=language==='de'?{p:'Bauer',n:'Springer',b:'Läufer',r:'Turm',q:'Dame'}:{p:'pawn',n:'knight',b:'bishop',r:'rook',q:'queen'};
 return <div className="review-material">{['p','n','b','r','q'].flatMap(r=>Array.from({length:counts[r]||0},(_,i)=><img key={r+i} src={`/pieces/neo/w${r}.png`} alt={`${language==='de'?'Geschlagen:':'Captured:'} ${roles[r as keyof typeof roles]}`}/>))}{difference>0&&<span>+{difference}</span>}</div>
}
