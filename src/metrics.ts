import {audioMetrics} from './audio';
const frames:number[]=[],longTasks:number[]=[],inputs:number[]=[],analysisTimes:number[]=[];let last=0;
function frame(t:number){if(last&&t-last<500)frames.push(t-last);if(frames.length>30000)frames.shift();last=t;requestAnimationFrame(frame)}requestAnimationFrame(frame);
try{new PerformanceObserver(list=>{for(const e of list.getEntries())longTasks.push(e.duration)}).observe({type:'longtask',buffered:true})}catch{}
export function inputFrame(start:number){requestAnimationFrame(()=>{inputs.push(performance.now()-start)})}
function percentiles(values:number[]){const s=values.slice().sort((a,b)=>a-b);return{samples:s.length,p50:s[Math.floor(s.length*.5)]??null,p95:s[Math.floor(s.length*.95)]??null,max:s.at(-1)??null}}
export function analysisFinished(start:number){analysisTimes.push(performance.now()-start)}
export function metrics(){return{analysis_response_ms:percentiles(analysisTimes),audio:audioMetrics(),frame_ms:percentiles(frames),input_to_next_raf_ms:percentiles(inputs),long_tasks_ms:percentiles(longTasks),viewport:{width:innerWidth,height:innerHeight,dpr:devicePixelRatio},note:'Frame intervals cover the session; input timing is pointer handler to next rAF, not hardware-to-photon latency. Analysis response covers the complete native grading IPC call, excluding voice generation. No external refresh-rate assumption.'}}

Object.assign(globalThis,{__coachMetrics:metrics});
