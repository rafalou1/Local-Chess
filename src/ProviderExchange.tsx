import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {Check,Copy,RefreshCw,Terminal,X} from 'lucide-react';
import {SelectField} from './SelectField';
import './providerExchange.css';

type Exchange={model:string;elapsed_ms:number;attempts:number;responses:string[];prompts:string[];raw_outputs:{text:string;truncated:boolean}[];outcome:string};
export function ProviderExchange({gameId,phase,provider,language,onClose}:{gameId:string;phase:string;provider:string;language:string;onClose:()=>void}){
 const de=language==='de',dialog=useRef<HTMLDialogElement>(null);
 const [rows,setRows]=useState<Exchange[]>([]),[index,setIndex]=useState(0),[attempt,setAttempt]=useState(0),[tab,setTab]=useState<'raw'|'prompt'>('raw'),[busy,setBusy]=useState(true),[error,setError]=useState(''),[copied,setCopied]=useState(false);
 useEffect(()=>{const el=dialog.current!,focused=document.activeElement as HTMLElement;el.showModal();return()=>{el.close();focused?.focus()}},[]);
 const [refresh,setRefresh]=useState(0);
 useEffect(()=>{let live=true;setBusy(true);setError('');void invoke<Exchange[]>('provider_diagnostics',{gameId}).then(data=>{if(live){setRows(data);setIndex(Math.max(0,data.length-1));setAttempt(0)}}).catch(e=>{if(live)setError(String(e))}).finally(()=>{if(live)setBusy(false)});return()=>{live=false}},[gameId,phase,refresh]);
 const row=rows[index],raw=row?.raw_outputs?.[attempt],text=tab==='raw'?raw?.text||'':row?.prompts?.[attempt]||'';
 useEffect(()=>{setCopied(false)},[index,attempt,tab]);
 useEffect(()=>{if(!copied)return;const timer=setTimeout(()=>setCopied(false),1600);return()=>clearTimeout(timer)},[copied]);
 async function copy(){try{await navigator.clipboard.writeText(text);setCopied(true);setError('')}catch{setError(de?'Kopieren fehlgeschlagen. Du kannst den Text markieren.':'Could not copy. You can select the text directly.')}}
 return <dialog ref={dialog} className="cli-exchange" role="dialog" onKeyDown={e=>e.stopPropagation()} aria-labelledby="cli-exchange-title" onCancel={e=>{e.preventDefault();onClose()}} onClick={e=>{if(e.target===dialog.current){const r=dialog.current.getBoundingClientRect();if(e.clientX<r.left||e.clientX>r.right||e.clientY<r.top||e.clientY>r.bottom)onClose()}}}>
  <header><div className="cli-exchange-icon"><Terminal size={21}/></div><div><span className="cli-eyebrow">{provider} · CLI</span><h2 id="cli-exchange-title">{de?'Hinter dem Zug':'Behind the move'}</h2></div><button autoFocus aria-label={de?'Antwortansicht schließen':'Close response viewer'} onClick={onClose}><X size={19}/></button></header>
  <div className="cli-exchange-toolbar"><div className="cli-exchange-picker"><SelectField label={de?'Zugantwort':'Move exchange'} disabled={!rows.length||busy} value={String(index)} options={rows.length?rows.map((r,i)=>({value:String(i),label:(de?'Zugantwort ':'Exchange ')+(i+1)+' · '+(r.elapsed_ms/1000).toFixed(2)+'s'})):[{value:'0',label:de?'Noch keine Antwort':'No exchange yet'}]} onChange={v=>{setIndex(+v);setAttempt(0)}}/></div><button disabled={busy} aria-label={de?'Antworten aktualisieren':'Refresh exchanges'} onClick={()=>setRefresh(n=>n+1)}><RefreshCw size={16}/></button></div>
  {row&&<div className="cli-exchange-meta"><span title={row.model}>{row.model}</span>{row.responses?.[attempt]&&<code>{row.responses[attempt]}</code>}</div>}
  {(row?.attempts||0)>1&&<div className="cli-attempts">{Array.from({length:row.attempts},(_,i)=><button key={i} aria-pressed={attempt===i} onClick={()=>setAttempt(i)}>{i===0?(de?'Erste Anfrage':'First request'):(de?'Korrektur':'Correction')}</button>)}</div>}
  <div className="cli-output"><div className="cli-output-bar"><div role="tablist" aria-label={de?'CLI-Inhalt':'CLI content'}>{(['raw','prompt'] as const).map(t=><button key={t} id={'cli-tab-'+t} role="tab" aria-selected={tab===t} aria-controls="cli-output-text" tabIndex={tab===t?0:-1} onClick={()=>setTab(t)} onKeyDown={e=>{if(['ArrowLeft','ArrowRight','Home','End'].includes(e.key)){e.preventDefault();const next=e.key==='Home'?'raw':e.key==='End'?'prompt':t==='raw'?'prompt':'raw';setTab(next);document.getElementById('cli-tab-'+next)?.focus()}}}>{t==='raw'?(de?'RAW-Antwort':'Raw response'):(de?'Gesendeter Prompt':'Sent prompt')}</button>)}</div><button disabled={!text} aria-label={de?'Angezeigten Text kopieren':'Copy displayed text'} onClick={()=>void copy()}>{copied?<Check size={14}/>:<Copy size={14}/>}<span>{copied?(de?'Kopiert':'Copied'):(de?'Kopieren':'Copy')}</span></button></div>
   <div id="cli-output-text" role="tabpanel" aria-labelledby={'cli-tab-'+tab} tabIndex={0}>{text?<pre>{text}</pre>:<div className="cli-empty"><Terminal size={28}/><p>{busy?(de?'Wird geladen…':'Loading…'):row?(de?'Für diesen Versuch wurde keine CLI-Ausgabe empfangen.':'No CLI output was received for this attempt.'):phase==='thinking'?(de?'Die Antwort erscheint, sobald der Zug beendet ist.':'The response appears when this turn finishes.'):de?'Spiele einen KI-Zug, um die Ausgabe zu sehen.':'Play an AI turn to see its output.'}</p></div>}</div>
  </div>
  {row&&row.outcome!=='Legal move validated'&&<p className="cli-exchange-error">{row.outcome}</p>}
  {error&&<p className="cli-exchange-error" role="alert">{error}</p>}
  <footer><span className="cli-status-dot"/><span>{tab==='raw'?(de?'CLI-Ereignisse, wie von der App empfangen':'CLI events as received by the app'):(de?'Der genaue Stellungsprompt dieses Versuchs':'The exact position prompt for this attempt')}</span>{tab==='raw'&&raw?.truncated&&<strong>{de?'Auf 256 KiB begrenzt':'Truncated at 256 KiB'}</strong>}</footer>
 </dialog>
}
