import {useEffect,useRef} from 'react';
import {xy,type Intent} from './Board';
export function PromotionPicker({intent,color,flip,onChoose,onCancel}:{intent:Intent;color:string;flip:boolean;onChoose:(role:string)=>void;onCancel:()=>void}){
 const ref=useRef<HTMLDivElement>(null);const [file,rank]=xy(intent.to,flip),top=rank===0;
 useEffect(()=>{ref.current?.querySelector('button')?.focus()},[]);
 return <div ref={ref} role="dialog" aria-modal="true" aria-label="Choose promotion" className={'promotion promotion-column '+(top?'top':'bottom')} style={{left:file*12.5+'%'}} onPointerDown={e=>e.stopPropagation()} onPointerUp={e=>e.stopPropagation()} onKeyDown={e=>{if(e.key==='Escape'){e.preventDefault();onCancel()}if(e.key==='Tab'){const buttons=Array.from(ref.current!.querySelectorAll('button'));const i=buttons.indexOf(document.activeElement as HTMLButtonElement);e.preventDefault();buttons[(i+(e.shiftKey?-1:1)+buttons.length)%buttons.length].focus()}}}>
 {['q','n','r','b'].map(p=><button key={p} aria-label={{q:'Promote to queen',n:'Promote to knight',r:'Promote to rook',b:'Promote to bishop'}[p]} onClick={()=>onChoose(p)}><img src={`/pieces/neo/${color}${p}.png`} alt={p}/></button>)}<button className="promotion-cancel" aria-label="Cancel" onClick={onCancel}>×</button>
 </div>
}
