import type {Game} from './types';
import positions from './opening-index.json';
const names:Record<string,{name:string}>=positions;
export function OpeningName({game,node,language='en'}:{game:Game;node:number;language?:string}){
 if(game.root!=='rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1'||game.chess960)return null;
 let n:number|null=node,name:string|undefined;
 while(n!==null){const current:Game['nodes'][number]=game.nodes[n];if(!current)break;name=names[current.fen.split(/\s+/).slice(0,4).join(' ')]?.name;if(name)break;n=current.parent;}
 return <div className="opening-name">{name||(language==='de'?'Anfangsstellung':'Starting Position')}</div>
}
