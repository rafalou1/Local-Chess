import type {Piece} from './types';

// Optimistic destinations for input while the opponent is moving. These are
// hints, not legal moves: the native rules validate the queue after the reply.
export function premoveTargets(pieces:Piece[],from:string,fen:string,chess960=false):string[]{
 const piece=pieces.find(p=>p.square===from);if(!piece)return [];
 const x=from.charCodeAt(0)-97,y=Number(from[1])-1,result:string[]=[];
 const square=(a:number,b:number)=>String.fromCharCode(97+a)+(b+1);
 const own=(a:number,b:number)=>pieces.some(p=>p.square===square(a,b)&&p.color===piece.color);
 const add=(a:number,b:number)=>{if(a>=0&&a<8&&b>=0&&b<8&&!own(a,b))result.push(square(a,b))};
 if(piece.role==='p'){
  const d=piece.color==='w'?1:-1;add(x,y+d);add(x-1,y+d);add(x+1,y+d);
  if(y===(piece.color==='w'?1:6)&&!own(x,y+d))add(x,y+2*d);
 }else if(piece.role==='n'){
  for(const [dx,dy] of [[1,2],[2,1],[-1,2],[-2,1],[1,-2],[2,-1],[-1,-2],[-2,-1]])add(x+dx,y+dy);
 }else if(piece.role==='k'){
  for(let dx=-1;dx<=1;dx++)for(let dy=-1;dy<=1;dy++)if(dx||dy)add(x+dx,y+dy);
  const rights=fen.split(' ')[2]||'',rank=piece.color==='w'?'1':'8';
  for(const right of rights){
   if((piece.color==='w')!==(right===right.toUpperCase()))continue;
   const file=right.toLowerCase()==='k'?'h':right.toLowerCase()==='q'?'a':right.toLowerCase();
   const rook=pieces.find(p=>p.square===file+rank&&p.role==='r'&&p.color===piece.color);
   if(!rook||from[1]!==rank)continue;
   const end=chess960?file+rank:(file.charCodeAt(0)>from.charCodeAt(0)?'g':'c')+rank;
   const low=Math.min(x,file.charCodeAt(0)-97),high=Math.max(x,file.charCodeAt(0)-97);
   if(!pieces.some(p=>p.color===piece.color&&p.square[1]===rank&&p.square.charCodeAt(0)-97>low&&p.square.charCodeAt(0)-97<high))result.push(end);
  }
 }else{
  const dirs=piece.role==='b'?[[1,1],[-1,1],[1,-1],[-1,-1]]:piece.role==='r'?[[1,0],[-1,0],[0,1],[0,-1]]:[[1,1],[-1,1],[1,-1],[-1,-1],[1,0],[-1,0],[0,1],[0,-1]];
  for(const [dx,dy] of dirs)for(let step=1;step<8;step++){const a=x+dx*step,b=y+dy*step;if(a<0||a>7||b<0||b>7||own(a,b))break;add(a,b)}
 }
 return [...new Set(result)];
}
