import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {Check,ChevronLeft,ChevronRight,Copy,Terminal,X} from 'lucide-react';
import './providerExchange.css';

type Exchange={responses:string[]};
export function ProviderExchange({gameId,phase,provider,language,onClose}:{gameId:string;phase:string;provider:string;language:string;onClose:()=>void}){
 const de=language==='de',dialog=useRef<HTMLDialogElement>(null);
 const [rows,setRows]=useState<Exchange[]>([]),[index,setIndex]=useState(0),[busy,setBusy]=useState(true),[error,setError]=useState(''),[copied,setCopied]=useState(false);
 useEffect(()=>{const el=dialog.current!,focused=document.activeElement as HTMLElement;el.showModal();return()=>{el.close();focused?.focus()}},[]);
 useEffect(()=>{let live=true;setBusy(true);setError('');void invoke<Exchange[]>('provider_diagnostics',{gameId}).then(data=>{if(live){const replies=data.filter(d=>d.responses.length);setRows(replies);setIndex(Math.max(0,replies.length-1))}}).catch(()=>{if(live)setError(de?'Antwort konnte nicht geladen werden.':'Could not load the response.')}).finally(()=>{if(live)setBusy(false)});return()=>{live=false}},[gameId,phase]);
 const text=rows[index]?.responses.at(-1)||'',move=/^[a-h][1-8][a-h][1-8][qrbn]?$/.test(text.trim());
 useEffect(()=>{setCopied(false)},[index]);
 useEffect(()=>{if(!copied)return;const timer=setTimeout(()=>setCopied(false),1600);return()=>clearTimeout(timer)},[copied]);
 async function copy(){try{await navigator.clipboard.writeText(text);setCopied(true);setError('')}catch{setError(de?'Kopieren fehlgeschlagen.':'Could not copy.')}}
 return <dialog ref={dialog} className="cli-exchange" role="dialog" onKeyDown={e=>e.stopPropagation()} aria-labelledby="cli-exchange-title" onCancel={e=>{e.preventDefault();onClose()}} onClick={e=>{if(e.target===dialog.current){const r=dialog.current.getBoundingClientRect();if(e.clientX<r.left||e.clientX>r.right||e.clientY<r.top||e.clientY>r.bottom)onClose()}}}>
  <header><div className="cli-exchange-icon"><Terminal size={20}/></div><div><span className="cli-eyebrow">{provider}</span><h2 id="cli-exchange-title">{de?'Antwort':'Response'}</h2></div><button autoFocus aria-label={de?'Antwortansicht schließen':'Close response viewer'} onClick={onClose}><X size={19}/></button></header>
  <div className={'cli-answer'+(move?' cli-answer-move':'')} tabIndex={0}>{text?<pre>{text}</pre>:<p role="status">{error|| (busy?(de?'Wird geladen…':'Loading…'):phase==='thinking'?(de?'Warte auf Antwort…':'Waiting for a response…'):(de?'Noch keine Antwort.':'No response yet.'))}</p>}</div>
  <footer><div className="cli-history">{rows.length>1&&<><button aria-label={de?'Vorherige Antwort':'Previous response'} disabled={index===0} onClick={()=>setIndex(i=>i-1)}><ChevronLeft size={16}/></button><span>{index+1} / {rows.length}</span><button aria-label={de?'Nächste Antwort':'Next response'} disabled={index===rows.length-1} onClick={()=>setIndex(i=>i+1)}><ChevronRight size={16}/></button></>}</div><button className="cli-copy" disabled={!text} aria-label={de?'Antwort kopieren':'Copy response'} onClick={()=>void copy()}>{copied?<Check size={15}/>:<Copy size={15}/>}<span>{copied?(de?'Kopiert':'Copied'):(de?'Kopieren':'Copy')}</span></button></footer>
  {text&&error&&<p className="cli-exchange-error" role="alert">{error}</p>}
 </dialog>
}
