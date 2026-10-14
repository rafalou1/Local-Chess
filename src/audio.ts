import type {Settings, Snapshot} from './types';

const names = ['move','opponent','capture','check','castle','promotion','end','invalid','start','lowtime'] as const;
type Sound = typeof names[number];
let context: AudioContext | undefined;
const buffers = new Map<Sound, AudioBuffer>();
const pending = new Map<Sound, Promise<AudioBuffer>>();
const measurements: {name:Sound|'great'|'brilliant';ready:boolean;scheduled_ms:number;output_latency_ms:number}[] = [];

function audioContext() {
  return context ??= new AudioContext({latencyHint:'interactive'});
}
function load(name:Sound) {
  let promise = pending.get(name);
  if (!promise) {
    promise = fetch(`/sounds/reference/${name}.mp3`)
      .then(r => {if (!r.ok) throw Error(`Sound unavailable: ${name}`); return r.arrayBuffer()})
      .then(b => audioContext().decodeAudioData(b))
      .then(buffer => {buffers.set(name,buffer);return buffer});
    pending.set(name,promise);
  }
  return promise;
}
/** Decode once, then reuse buffers. User gestures resume the Windows audio device. */
export function prepareAudio() {
  for (const name of names) void load(name).catch(()=>{});
  const unlock=()=>{void audioContext().resume().catch(()=>{});};
  document.addEventListener('pointerdown',unlock,{capture:true});
  document.addEventListener('keydown',unlock,{capture:true});
  return ()=>{document.removeEventListener('pointerdown',unlock,true);document.removeEventListener('keydown',unlock,true)};
}
export function sound(name:Sound, settings:Settings) {
  if (settings.mute || settings.volume<=0) return;
  const start=performance.now(), ready=buffers.has(name), c=audioContext();
  void c.resume().then(async()=>{
    const buffer=buffers.get(name) ?? await load(name);
    const source=c.createBufferSource(),gain=c.createGain();
    source.buffer=buffer;gain.gain.value=settings.volume;
    source.connect(gain);gain.connect(c.destination);
    source.onended=()=>{source.disconnect();gain.disconnect()};
    source.start();
    measurements.push({name,ready,scheduled_ms:performance.now()-start,output_latency_ms:1000*(c.baseLatency+(c.outputLatency||0))});
    if(measurements.length>200)measurements.shift();
  }).catch(()=>{});
}
/** Short, distinct local feedback cues for exceptional review moves. These are
 * synthesized locally so playback starts immediately and works offline. */
export function playMoveGradeCue(label:string,settings:Settings){
  if(settings.mute||settings.volume<=0||!['Great','Brilliant'].includes(label))return;
  const c=audioContext();void c.resume().then(()=>{
    const brilliant=label==='Brilliant',notes=brilliant?[523.25,659.25,783.99,1046.5]:[659.25,880],now=c.currentTime+.012;
    notes.forEach((hz,i)=>{const osc=c.createOscillator(),gain=c.createGain(),start=now+i*(brilliant?.052:.075),duration=brilliant?.19:.22;osc.type=brilliant?'sine':'triangle';osc.frequency.setValueAtTime(hz,start);gain.gain.setValueAtTime(.0001,start);gain.gain.exponentialRampToValueAtTime(settings.volume*(brilliant?.105:.09),start+.012);gain.gain.exponentialRampToValueAtTime(.0001,start+duration);osc.connect(gain);gain.connect(c.destination);osc.start(start);osc.stop(start+duration+.015);osc.onended=()=>{osc.disconnect();gain.disconnect()}});
    measurements.push({name:brilliant?'brilliant':'great',ready:true,scheduled_ms:0,output_latency_ms:1000*(c.baseLatency+(c.outputLatency||0))});if(measurements.length>200)measurements.shift();
  }).catch(()=>{});
}
export function moveSound(next:Snapshot, opponent=false):Sound {
  const n=next.game.nodes[next.node];
  // Exactly one sound per authoritative move. End > check > promotion > capture > castle.
  return next.status?'end':next.check?'check':n.uci.length===5?'promotion':n.san.includes('x')?'capture':n.san.startsWith('O-')?'castle':opponent?'opponent':'move';
}
export function audioMetrics(){return{state:context?.state??'uninitialized',decoded:buffers.size,events:measurements,note:'Scheduling and reported audio-device latency, not microphone-measured acoustic latency.'}}
let coachTicket=0,coachNext=0;const coachNodes=new Set<{source:AudioBufferSourceNode;gain:GainNode}>();
export function stopCoachAudio(){coachTicket++;coachNext=0;for(const n of coachNodes){try{n.source.stop()}catch{}n.gain.disconnect()}coachNodes.clear()}
export function adjustCoachVolume(volume:number){if(context)for(const n of coachNodes)n.gain.gain.setTargetAtTime(volume,context.currentTime,.02)}
export async function playCoachAudio(base64:string,volume:number,append=false){if(!append)stopCoachAudio();const ticket=coachTicket,c=audioContext();await c.resume();const bytes=Uint8Array.from(atob(base64),v=>v.charCodeAt(0));const buffer=await c.decodeAudioData(bytes.buffer);const playableAt=performance.now();if(ticket!==coachTicket)return;const source=c.createBufferSource(),gain=c.createGain(),node={source,gain};coachNodes.add(node);source.buffer=buffer;gain.gain.value=volume;source.connect(gain);gain.connect(c.destination);let ended:()=>void=()=>{};const finished=new Promise<void>(resolve=>ended=resolve);source.onended=()=>{source.disconnect();gain.disconnect();coachNodes.delete(node);ended()};const start=Math.max(c.currentTime+.01,coachNext);coachNext=start+buffer.duration;source.start(start);return {finished,playableAt};}

