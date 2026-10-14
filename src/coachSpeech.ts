import {listen} from '@tauri-apps/api/event';
import {invoke} from '@tauri-apps/api/core';
import {adjustCoachVolume,playCoachAudio,stopCoachAudio} from './audio';
import type {Settings} from './types';
let ticket=Date.now()*1000,preferences:Settings|undefined;
let statusOwner:((s:string)=>void)|undefined;
const localized=(en:string,de:string)=>preferences?.language==='de'?de:en;
const idle=()=>localized('Coach voice ready','Coach-Stimme bereit');
type Metric={first_audio_scheduled_ms?:number;first_playable_audio_ms?:number;chunks:number;provider:string;generation_ms:number;total_ms:number;completion_ms?:number;cache_hit:boolean;voice:string;cancelled?:boolean};
const events:Metric[]=[];
type Stream={id:number;seen:Set<number>;received:number;buffer:Map<number,string>;chunks:number;first_ms:number|null;playable_ms:number|null;started:number;pending:Promise<void>;finished:Promise<void>[];status:(s:string)=>void;error?:unknown};
let activeStream:Stream|undefined,streamListener:Promise<unknown>|undefined;
function ensureStreamListener(){return streamListener??=listen<{sequence:number;index:number;wav_base64:string}>('neural-audio',({payload:p})=>{
 const stream=activeStream;if(!stream||p.sequence!==ticket||stream.id!==ticket||stream.seen.has(p.index)||p.index<stream.chunks)return;
 stream.seen.add(p.index);stream.received++;stream.buffer.set(p.index,p.wav_base64);
 stream.pending=stream.pending.then(async()=>{while(stream.id===ticket&&stream.buffer.has(stream.chunks)){
  const wav=stream.buffer.get(stream.chunks)!;stream.buffer.delete(stream.chunks);
  const playback=await playCoachAudio(wav,preferences?.voiceVolume??.75,true);if(stream.id!==ticket)return;
  stream.chunks++;stream.first_ms??=performance.now()-stream.started;stream.playable_ms??=playback?playback.playableAt-stream.started:null;
  stream.status(localized('Speaking…','Spricht…'));if(playback)stream.finished.push(playback.finished);
 }}).catch(e=>{stream.error=e});
});}
let queueEpoch=0,queue=Promise.resolve();
export function queueCoachSpeech(text:string,onStatus:(s:string)=>void){const epoch=queueEpoch;queue=queue.catch(()=>{}).then(async()=>{if(epoch===queueEpoch)await speakCoach(text,onStatus,false,true)});}
export function stopCoachSpeech(){queueEpoch++;queue=Promise.resolve();activeStream=undefined;ticket++;stopCoachAudio();statusOwner?.(preferences?.voice&&preferences.voiceVolume>0?idle():localized('Coach voice muted','Coach-Stimme stumm'));statusOwner=undefined;void invoke('cancel_neural',{sequence:ticket}).catch(()=>{});void invoke('speak_local',{text:'',language:preferences?.language??'en',volume:0}).catch(()=>{});}
export function configureCoachSpeech(next:Settings){const old=preferences;preferences=next;adjustCoachVolume(next.voiceVolume);if(old&&((old.voice&&!next.voice)||(old.voiceVolume>0&&next.voiceVolume===0)||old.voiceProvider!==next.voiceProvider||old.neuralVoice!==next.neuralVoice||old.voiceSpeed!==next.voiceSpeed||old.language!==next.language))stopCoachSpeech();}
export async function warmCoachVoice(){const s=preferences;if(s?.voice&&s.voiceProvider!=='windows'&&!statusOwner)await invoke('neural_speech',{text:'',voice:s.language==='de'?102:s.neuralVoice,speed:s.voiceSpeed,sequence:ticket});}
async function speakWindows(text:string,s:Settings,id:number,onStatus:(s:string)=>void){
 const started=performance.now();const r=await invoke<{voice:string}>('speak_local',{text,language:s.language,volume:s.voiceVolume});if(id!==ticket)return;
 onStatus(localized('Speaking · ','Spricht · ')+r.voice);
 const metric:Metric={chunks:0,provider:'Windows SAPI',generation_ms:performance.now()-started,total_ms:0,cache_hit:false,voice:r.voice};events.push(metric);
 const deadline=performance.now()+120000;
 while(id===ticket&&performance.now()<deadline){await new Promise(resolve=>setTimeout(resolve,100));if(id!==ticket)break;const state=await invoke<{speaking:boolean}>('speak_local',{text:'',language:s.language,volume:s.voiceVolume,query:true});if(!state.speaking){metric.completion_ms=performance.now()-started;break;}}
 metric.total_ms=performance.now()-started;metric.cancelled=id!==ticket;
}
export async function speakCoach(text:string,onStatus:(s:string)=>void,force=false,queued=false){
 if(!queued){queueEpoch++;queue=Promise.resolve()}
 const s=preferences;if(!s||(!s.voice&&!force)||s.voiceVolume<=0)return;
 let id=++ticket;statusOwner=onStatus;stopCoachAudio();activeStream=undefined;
 void invoke('speak_local',{text:'',language:s.language,volume:0}).catch(()=>{});const started=performance.now();
 try{
  if(s.voiceProvider!=='windows'){
   onStatus(localized('Preparing voice…','Stimme wird vorbereitet…'));await ensureStreamListener();if(id!==ticket)return;
   const stream:Stream={id,seen:new Set(),received:0,buffer:new Map(),chunks:0,first_ms:null,playable_ms:null,started,pending:Promise.resolve(),finished:[],status:onStatus};activeStream=stream;
   const generation=invoke<{stream_chunks:number;provider:string;wav_base64:string;generation_ms:number;cache_hit:boolean;voice:string}>('neural_speech',{text:text.slice(0,600),voice:s.language==='de'?102:s.neuralVoice,speed:s.voiceSpeed,sequence:id,stream:true});
   let timer:ReturnType<typeof setTimeout>|undefined;
   const onset=new Promise<never>((_,reject)=>{timer=setTimeout(()=>{if(id===ticket&&!stream.chunks)reject(Error('Fast voice onset timed out'))},1200)});
   let a;try{a=await (s.voiceProvider==='fast'||s.language==='de'?Promise.race([generation,onset]):generation)}finally{if(timer)clearTimeout(timer)}
   if(id!==ticket||!a)return;
   const fence=performance.now()+3000;while(stream.received<a.stream_chunks&&id===ticket&&performance.now()<fence)await new Promise(resolve=>setTimeout(resolve,5));await stream.pending;if(id!==ticket)return;activeStream=undefined;
   if(stream.error)throw stream.error;if(stream.chunks!==a.stream_chunks)throw Error('Incomplete speech stream');
   const playback=stream.chunks?undefined:await playCoachAudio(a.wav_base64,preferences?.voiceVolume??s.voiceVolume);if(id!==ticket){stopCoachAudio();return}
   const metric:Metric={first_audio_scheduled_ms:stream.first_ms??performance.now()-started,first_playable_audio_ms:stream.playable_ms??(playback?playback.playableAt-started:undefined),chunks:stream.chunks,provider:a.provider,generation_ms:a.generation_ms,total_ms:0,cache_hit:a.cache_hit,voice:a.voice};events.push(metric);if(events.length>100)events.shift();
   onStatus(localized('Speaking · ','Spricht · ')+a.voice);await playback?.finished;await Promise.all(stream.finished);metric.total_ms=performance.now()-started;metric.cancelled=id!==ticket;if(id===ticket)metric.completion_ms=metric.total_ms;
  }else await speakWindows(text,s,id,onStatus);
 }catch(e){
  if(id!==ticket||String(e).includes('cancelled'))return;activeStream=undefined;stopCoachAudio();id=++ticket;void invoke('cancel_neural',{sequence:id}).catch(()=>{});
  onStatus(localized('Voice unavailable; preparing Windows fallback…','Stimme nicht verfügbar; Windows-Ersatz wird vorbereitet…'));
  try{await speakWindows(text,s,id,onStatus)}catch(err){if(id===ticket){onStatus(String(err));statusOwner=undefined;id=-1;return}}
 }finally{if(id===ticket){statusOwner=undefined;onStatus(idle())}}
}
export function speechMetrics(){return{events,note:'Playable means decoded audio; scheduled means WebAudio start scheduled. Completion awaits playback/SAPI status. No microphone-measured audible onset.'}}
