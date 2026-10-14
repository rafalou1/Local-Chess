import {ProviderAvatar,ProviderSetup,ProviderTurnPanel,providerName,type ProviderTurn} from './ProviderPlay';

import type {AISelection} from './types';

import {LearningRoom} from './LearningRoom';import {ThreatLesson,WeaknessNudge} from './ThreatLesson';

import {ReviewMaterial} from './ReviewMaterial';



import {GameReview,type ReviewReport,type ReviewVisual} from './GameReview';import {ResultDialog} from './ResultDialog';



import {CoachVolume} from './CoachVolume';import {SettingsPanel} from './SettingsPanel';import {configureCoachSpeech,speakCoach,queueCoachSpeech,stopCoachSpeech,warmCoachVoice,speechMetrics} from './coachSpeech';



import {MoveSan} from './MoveSan';import {OpeningName} from './OpeningName';



import {PromotionPicker} from './PromotionPicker';



import {analysisFinished} from './metrics';

import {listen} from '@tauri-apps/api/event';import {metrics} from './metrics';import {PositionEditor} from './PositionEditor';import {ProviderSettings} from './ProviderSettings';







import React,{useEffect,useRef,useState} from 'react';import{createRoot}from'react-dom/client';import{invoke}from'@tauri-apps/api/core';







import{BookOpen,ChartNoAxesCombined,ChevronDown,ChevronLeft,ChevronRight,ChevronsLeft,ChevronsRight,Flag,FlipVertical,GraduationCap,Home,Lightbulb,Pause,Play,RotateCcw,Settings as Gear,Volume2,VolumeX,FolderOpen,Download,Upload,Maximize,Swords,Check,HelpCircle,X}from'lucide-react';







import {sound,moveSound,playMoveGradeCue,prepareAudio} from './audio';import {MoveBadge} from './MoveBadge';import './style.css';import './settings.css';import './review.css';import './polish.css';import{Board,type Intent}from'./Board';import{defaults,type Settings,type Snapshot,type Analysis,type Line}from'./types';







function Avatar(){return <img src="/coach-reference.png" className="avatar" alt="Coach"/>}















const clock=(ms:number,base:number)=>!base?'∞':`${Math.floor(ms/60000)}:${Math.floor(ms/1000%60).toString().padStart(2,'0')}${ms<10000?'.'+Math.floor(ms/100%10):''}`;







type ReviewPreview={snapshot:Snapshot;search:Analysis['search'];analysis?:{label:string|null;explanation:string}|null};

type ManualSearch={args:{gameId:string;revision:number;node:number;moves:string[]};token:number};

type ManualLine={origin:number;moves:string[];cursor:number;sans?:string[]};



const interfaceDE:Record<string,string>={"Main navigation": "Navigation", "Play coach": "Gegen den Coach spielen", "Saved games": "Gespeicherte Partien", "Game review": "Partieanalyse", "Analysis board": "Analysebrett", "Offline training": "Offline-Training", "Focus mode": "Fokusmodus", "Mute": "Stummschalten", "Settings": "Einstellungen", "Board settings": "Bretteinstellungen", "Coach settings": "Coach-Einstellungen", "Play vs. Coach": "Gegen den Coach spielen", "Offline Training": "Offline-Training", "Analysis": "Analyse", "Casual": "Entspannt", "Balanced": "Ausgeglichen", "Challenging": "Herausfordernd", "Play As:": "Spielen als:", "An easier game so you can focus on learning.": "Eine leichtere Partie, bei der du dich aufs Lernen konzentrieren kannst.", "An even match to play while you learn.": "Eine ausgeglichene Partie zum Spielen und Lernen.", "A harder game that pushes you to compete and learn.": "Eine anspruchsvolle Partie, die dich herausfordert.", "Show All Levels": "Alle Spielstärken", "Maximum": "Maximal", "Local estimates. Beginner profiles are not calibrated ratings.": "Lokale Schätzungen. Anfängerprofile entsprechen keinen kalibrierten Wertungen.", "Game options": "Partieoptionen", "Time control": "Bedenkzeit", "Untimed learning": "Lernen ohne Zeitlimit", "Increment (seconds)": "Zeitbonus (Sekunden)", "Language": "Sprache", "Starting position / FEN": "Startstellung / FEN", "Offline · no account needed": "Offline · kein Konto nötig", "Review game": "Partie analysieren", "Rematch / new game": "Neue Partie", "Move history": "Zugliste", "Your moves will appear here.": "Hier erscheinen deine Züge.", "Node comment": "Stellungskommentar", "Comment on this position": "Kommentar zu dieser Stellung", "First move": "Erster Zug", "Previous move": "Vorheriger Zug", "Autoplay": "Automatische Wiedergabe", "Next move": "Nächster Zug", "Live position": "Aktuelle Stellung", "Return to live game": "Zurück zur Partie", "Analysis details": "Analysedetails", "Ask your coach": "Frag deinen Coach", "Why?": "Warum?", "Threat?": "Drohung?", "Plan?": "Plan?", "Show line": "Variante zeigen", "Explore / retry": "Ausprobieren", "Resign": "Aufgeben", "Hint": "Hinweis", "Takeback": "Zurücknehmen", "Resume": "Fortsetzen", "Pause": "Pause", "Offer draw": "Remis anbieten", "Export game": "Partie exportieren", "Dismiss": "Schließen", "Close dialog": "Dialog schließen", "White": "Weiß", "Black": "Schwarz", "Random": "Zufällig", "Same-PC two player": "Zwei Spieler am PC", "You": "Du", "White to move": "Weiß am Zug", "Black to move": "Schwarz am Zug", "Viewing history · live game continues": "Zugverlauf · die Partie läuft weiter", "Position editor": "Stellungseditor", "Start from position": "Mit dieser Stellung starten", "Search games": "Partien suchen", "Restore backup": "Sicherung wiederherstellen", "Copy FEN": "FEN kopieren", "Confirm": "Bestätigen", "Keep playing": "Weiterspielen", "Resign this game?": "Diese Partie aufgeben?", "Agree to a draw?": "Remis vereinbaren?"};

function App(){const [opponentMode,setOpponentMode]=useState('local'),[aiSelection,setAiSelection]=useState<AISelection|null>(null),[providerReady,setProviderReady]=useState(false),[providerBlockReason,setProviderBlockReason]=useState(''),[startError,setStartError]=useState(''),[startingGame,setStartingGame]=useState(false),[providerTurn,setProviderTurn]=useState<ProviderTurn>({phase:'idle'}),[providerRetry,setProviderRetry]=useState(0);const [learningGameId,setLearningGameId]=useState<string|undefined>(),[learningTheme,setLearningTheme]=useState(''),[learningKind,setLearningKind]=useState('puzzles'),[threatPreview,setThreatPreview]=useState<Snapshot|null>(null);const manualSearchRunning=useRef(false),manualSearchPending=useRef<ManualSearch|null>(null);const[manualBranches,setManualBranches]=useState<ManualLine[]>([]);const[reviewInputEpoch,setReviewInputEpoch]=useState(0);const[manualLine,setManualLine]=useState<ManualLine|null>(null);const manualRef=useRef<ManualLine|null>(null),previewRef=useRef<ReviewPreview|null>(null),manualGeneration=useRef(0);const[reviewPreview,setReviewPreview]=useState<ReviewPreview|null>(null);const[reviewReport,setReviewReport]=useState<ReviewReport|null>(null),[reviewVisual,setReviewVisual]=useState<ReviewVisual>({}),[showResult,setShowResult]=useState(false);manualRef.current=manualLine;previewRef.current=reviewPreview;const endedGame=useRef('');const[setupPreview,setSetupPreview]=useState<Snapshot|null>(null);const[snap,setSnap]=useState<Snapshot|null>(null),[settings,setSettings]=useState(defaults),[setup,setSetup]=useState(true),[flip,setFlip]=useState(false),[mode,setMode]=useState('play'),[modal,setModal]=useState(''),[error,setError]=useState(''),[analysis,setAnalysis]=useState<Analysis|null>(null),[engineStatus,setEngineStatus]=useState('Ready · native Stockfish 19'),[queue,setQueue]=useState<Intent[]>([]),[promotion,setPromotion]=useState<{intent:Intent;alt:boolean;premove?:boolean}|null>(null),[hint,setHint]=useState(0),[busy,setBusy]=useState(false),[level,setLevel]=useState(400),[human,setHuman]=useState('w'),[time,setTime]=useState(0),[increment,setIncrement]=useState(0),[fen,setFen]=useState(''),[showLevels,setShowLevels]=useState(false),[archive,setArchive]=useState<{id:string;name:string;result:string|null;moves:number}[]>([]),[filter,setFilter]=useState(''),[diagnostics,setDiagnostics]=useState(''),[importText,setImportText]=useState(''),[autoplay,setAutoplay]=useState(false),[training,setTraining]=useState(0),[trainingMessage,setTrainingMessage]=useState(''),[coachQuestion,setCoachQuestion]=useState('');







 useEffect(()=>{void invoke('warm_engines').catch(()=>{});return prepareAudio()},[]);







 const [evidence,setEvidence]=useState<Record<string,Analysis>>({});







 const [showEngineDetails,setShowEngineDetails]=useState(false);const [coachMessages,setCoachMessages]=useState<Record<string,string>>({});const [coachMessage,setCoachMessage]=useState(''),[voiceStatus,setVoiceStatus]=useState('Natural offline voice ready');const spoken=useRef(''),evaluationRef=useRef<{gameId:string;line:Line|null}>({gameId:'',line:null});







 const [autoCommit,setAutoCommit]=useState(false),[premoveTransition,setPremoveTransition]=useState<{gameId:string;from:number;to:number}|undefined>();

 const live=useRef<Snapshot|null>(null),sequence=useRef(0),opponentRequest=useRef(''),opponentGeneration=useRef(0),queueRef=useRef(queue),inputLock=useRef(false),analysisRequest=useRef('');queueRef.current=queue;const modeRef=useRef(mode);modeRef.current=mode;







 const [mistakeWarning,setMistakeWarning]=useState<{gameId:string;revision:number;node:number;arrow?:string}|null>(null);

 const [referenceEvaluation,setReferenceEvaluation]=useState<{gameId:string;fen:string;line:Line}|null>(null),[referenceStatus,setReferenceStatus]=useState('');

 const visibleEvaluationFen=(mode==='review'?reviewPreview?.snapshot:threatPreview)?.fen||snap?.fen;

 useEffect(()=>{if(!snap||setup||!settings.eval||settings.evaluationProfile!=='reference-lite'||!visibleEvaluationFen)return;let active=true;setReferenceStatus(settings.language==='de'?'Lite-Bewertung läuft…':'Checking Lite evaluation…');void invoke<{game_id:string;revision:number;fen:string;search:Analysis['search']}>('evaluate_reference',{gameId:snap.game.id,revision:snap.game.revision,fen:visibleEvaluationFen,budget:150}).then(r=>{if(active&&r.search.lines[0]){setReferenceEvaluation({gameId:r.game_id,fen:r.fen,line:r.search.lines[0]});setReferenceStatus('Stockfish 19 Lite · d'+r.search.lines[0].depth)}}).catch(e=>{if(active&&!String(e).includes('Stale'))setReferenceStatus(String(e))});return()=>{active=false}},[snap?.game.id,snap?.game.revision,visibleEvaluationFen,setup,settings.eval,settings.evaluationProfile,settings.language]);

 const warningRef=useRef<typeof mistakeWarning>(null),warningSeen=useRef(new Set<string>()),moveListRef=useRef<HTMLDivElement>(null),contextRef=useRef({setup,mode,settings});contextRef.current={setup,mode,settings};

 const clearWarning=()=>{warningRef.current=null;setMistakeWarning(null)};

 const cancelOpponent=()=>{opponentGeneration.current++;opponentRequest.current='';if(live.current?.game.ai_opponent)void invoke('provider_cancel',{key:live.current.game.id});};

 const cancelProviderTurn=()=>{const id=live.current?.game.id;cancelOpponent();if(id)opponentRequest.current=id+':'+live.current!.game.revision;setProviderTurn({phase:'cancelled',message:'Turn cancelled. Position unchanged. Retry when ready.'})};

 const switchToLocal=async(skip=false)=>{const s=live.current;if(!s)return;cancelOpponent();try{const next=await invoke<Snapshot>('provider_local',{gameId:s.game.id,revision:s.game.revision,skip});update(next);setProviderTurn({phase:'idle'});setQueue([])}catch(e){fail(e)}};

 useEffect(()=>{if(!settings.pauseMistakes||settings.quiet||mode!=='play'||setup)clearWarning()},[settings.pauseMistakes,settings.quiet,mode,setup]);

 useEffect(()=>{const list=moveListRef.current;if(!list)return;let frame=0;const keepCurrentVisible=()=>{cancelAnimationFrame(frame);frame=requestAnimationFrame(()=>{const current=list.querySelector<HTMLElement>('.current');if(!current)return;const top=current.getBoundingClientRect().top-list.getBoundingClientRect().top+list.scrollTop;if(top<list.scrollTop)list.scrollTop=top;else if(top+current.offsetHeight>list.scrollTop+list.clientHeight)list.scrollTop=top+current.offsetHeight-list.clientHeight;})};keepCurrentVisible();const resize=new ResizeObserver(keepCurrentVisible);resize.observe(list);const mutation=new MutationObserver(keepCurrentVisible);mutation.observe(list,{childList:true,subtree:true});return()=>{cancelAnimationFrame(frame);resize.disconnect();mutation.disconnect()}},[snap?.node,snap?.game.id,snap?.game.live,setup,mode]);

 const update=(s:Snapshot)=>{if(warningRef.current&&(warningRef.current.gameId!==s.game.id||warningRef.current.revision!==s.game.revision||warningRef.current.node!==s.node))clearWarning();live.current=s;setSnap(s);setError('')};const fail=(e:unknown)=>setError(String(e));







 useEffect(()=>{if(!snap||setup||mode==='review'||mode==='training')return;let alive=true;const nodes:number[]=[];let at:number|null=snap.node;while(at!==null&&nodes.length<2){if(at>0&&snap.game.nodes[at].evidence)nodes.push(at);at=snap.game.nodes[at].parent}void invoke<Record<string,string>>('coach_messages',{gameId:snap.game.id,revision:snap.game.revision,nodes}).then(messages=>{if(alive)setCoachMessages(old=>({...old,...messages}))}).catch(()=>{});return()=>{alive=false}},[snap?.game.id,snap?.game.revision,snap?.node,settings.language,setup,mode]);

 useEffect(()=>{void Promise.all([invoke<Snapshot>('snapshot'),invoke<string>('preferences'),invoke<Snapshot>('preview_position')]).then(([s,p,preview])=>{setSetupPreview(preview);update(s);const saved=JSON.parse(p);const migrated={...defaults,...saved,...(!saved.speechVersion?{voice:true,speechVersion:1}:{}),...(saved.neuralVersion!==2?{voiceProvider:'fast',neuralVoice:100,voiceSpeed:1,neuralVersion:2}:{})};setSettings(migrated);if(!saved.speechVersion||saved.neuralVersion!==2)void invoke('preferences',{value:JSON.stringify(migrated)});if(s.game.nodes.length>1||s.game.ai_opponent){setSetup(false);setFlip(s.game.human==='b');setHuman(s.game.human)}}).catch(fail);return()=>stopCoachSpeech();},[]);







 useEffect(()=>{let off:(()=>void)|undefined;void listen<Analysis>('quick-analysis',({payload:a})=>{const s=live.current;if(s&&a.game_id===s.game.id&&a.revision===s.game.revision&&a.node===s.node&&a.sequence===sequence.current){setAnalysis(a);setEngineStatus(`Provisional · depth ${a.search.lines[0]?.depth||0} · ${Math.round(a.search.elapsed_ms)} ms`);}}).then(f=>off=f).catch(fail);const timer=setInterval(()=>void invoke('record_metrics',{data:metrics()}).catch(()=>{}),15000);return()=>{off?.();clearInterval(timer)}},[]);







 const feedbackReady=useRef(new Set<string>()),gradeCuePlayed=useRef(new Set<string>());useEffect(()=>{configureCoachSpeech(settings)},[settings]);useEffect(()=>{let active=true;const de=settings.language==='de';const ready=!settings.voice?(de?'Coach-Stimme stumm':'Coach voice muted'):settings.voiceProvider==='windows'?(de?'Windows-Stimme bereit':'Windows voice ready'):(de?'Thorsten · lokale Stimme bereit':settings.voiceProvider==='fast'?'Fast local voice ready':'Expressive local voice ready');const neural=settings.voice&&settings.voiceProvider!=='windows';setVoiceStatus(neural?(de?'Lokale Stimme wird geladen…':'Warming local voice…'):ready);void warmCoachVoice().then(()=>{if(active)setVoiceStatus(ready)}).catch(e=>{if(active)setVoiceStatus((de?'Lokale Stimme nicht verfügbar: ':'Natural voice unavailable: ')+String(e))});return()=>{active=false}},[settings.voiceProvider,settings.neuralVoice,settings.language,settings.voice]);



 const lowTimeWarned=useRef(new Set<string>());useEffect(()=>{if(!snap||setup||snap.paused||snap.status||!snap.game.base_ms||snap.game.human==='both')return;const ms=snap.game.human==='w'?snap.white_ms:snap.black_ms,key=snap.game.id+':'+snap.game.human;if(ms<=10000&&ms>0&&!lowTimeWarned.current.has(key)){lowTimeWarned.current.add(key);sound('lowtime',settings)}},[snap?.white_ms,snap?.black_ms,snap?.game.id,setup]);



 const changeSettings=(patch:Partial<Settings>)=>{const next={...settings,...patch};setSettings(next);void invoke('preferences',{value:JSON.stringify(next)}).catch(fail);};







 function returnToRecorded(){manualSearchPending.current=null;const old=manualRef.current,hadPreview=!!previewRef.current;if(old?.moves.length)setManualBranches(lines=>[...lines.filter(l=>l.origin!==old.origin||l.moves.join(' ')!==old.moves.join(' ')),old]);setReviewInputEpoch(v=>v+1);manualGeneration.current++;manualRef.current=null;setManualLine(null);setReviewPreview(null);if(old||hadPreview)setReviewVisual({});setPromotion(null);stopCoachSpeech();}



 async function pumpManualSearch(){if(manualSearchRunning.current)return;manualSearchRunning.current=true;try{while(manualSearchPending.current){const job=manualSearchPending.current;manualSearchPending.current=null;if(job.token!==manualGeneration.current)continue;try{const p=await invoke<ReviewPreview>('review_line',{...job.args,classify:true});if(job.token===manualGeneration.current&&modeRef.current==='review'&&live.current?.game.id===job.args.gameId&&live.current.game.revision===job.args.revision){setReviewPreview(p);setReviewVisual(p.search.best&&p.snapshot.legal.includes(p.search.best)?{arrow:p.search.best}:{})}}catch(e){if(job.token===manualGeneration.current&&!String(e).includes('Stale'))fail(e)}}}finally{manualSearchRunning.current=false}}

 async function showManual(line:ManualLine,fromInput=false){if(!fromInput)setReviewInputEpoch(v=>v+1);const s=live.current;if(!s)return;const token=++manualGeneration.current;inputLock.current=true;setBusy(true);setPromotion(null);stopCoachSpeech();setReviewVisual({});try{const args={gameId:s.game.id,revision:s.game.revision,node:line.origin,moves:line.moves.slice(0,line.cursor)};const snapshot=await invoke<Snapshot>('review_variation',args);if(token!==manualGeneration.current||modeRef.current!=='review'||live.current?.game.id!==s.game.id||live.current.game.revision!==s.game.revision)return;const completed={...line,sans:line.moves.map((u,i)=>i<line.cursor?snapshot.game.nodes[snapshot.game.nodes.length-line.cursor+i].san:line.sans?.[i]||u)};manualRef.current=completed;setManualLine(completed);const previous=previewRef.current?.search||{lines:[],best:'',elapsed_ms:0,engine:'Stockfish 19'};setReviewPreview({snapshot,search:previous});sound(moveSound(snapshot),settings);manualSearchPending.current={args,token};void pumpManualSearch();}catch(e){if(token===manualGeneration.current){sound('invalid',settings);fail(e)}}finally{inputLock.current=false;setBusy(false)}}



 async function resumeManual(line:ManualLine,cursor:number){if(inputLock.current)return;inputLock.current=true;try{await view(line.origin);await showManual({...line,cursor});}finally{inputLock.current=false;}}

 useEffect(()=>{setManualBranches([])},[snap?.game.id]);

 async function commitManual(intent:Intent,alt:boolean){const original=live.current,s=previewRef.current?.snapshot||original;if(!original||!s||inputLock.current)return;const legal=s.legal.filter(m=>m.startsWith(intent.from+intent.to));if(!legal.length){sound('invalid',settings);return;}if(legal.some(m=>m.length===5)&&!intent.promotion){if(settings.autoQueen&&!alt)intent={...intent,promotion:'q'};else{setPromotion({intent,alt});return;}}const uci=intent.from+intent.to+(intent.promotion||'');const existing=manualRef.current;if(!existing&&!previewRef.current){const path:number[]=[];let n=original.game.live;while(n!==original.node&&original.game.nodes[n].parent!==null){path.unshift(n);n=original.game.nodes[n].parent!;}if(n===original.node&&path.length&&original.game.nodes[path[0]].uci===uci){sound('move',settings);await view(path[0]);return;}}



 let prefix=existing?.moves.slice(0,existing.cursor)||[];if(!existing&&previewRef.current){let n=s.node;while(n!==original.node&&s.game.nodes[n]?.parent!==null){prefix.unshift(s.game.nodes[n].uci);n=s.game.nodes[n].parent!;}}



 if(existing&&existing.cursor<existing.moves.length)setManualBranches(lines=>[...lines.filter(l=>l.origin!==existing.origin||l.moves.join(' ')!==existing.moves.join(' ')),existing]);const moves=[...prefix,uci];await showManual({origin:existing?.origin??original.node,moves,cursor:moves.length,sans:existing?.sans?.slice(0,existing.cursor)},true);}



 async function commit(intent:Intent,alt=false,premove=false){if(modeRef.current==='review'){await commitManual(intent,alt);return;}const s=live.current;if(!s||inputLock.current||warningRef.current||(s.node!==s.game.live&&s.game.human!=='both'))return;const legal=s.legal.filter(m=>m.startsWith(intent.from+intent.to));if(!legal.length){sound('invalid',settings);return}if(legal.some(m=>m.length===5)&&!intent.promotion){if(settings.autoQueen&&!alt)intent={...intent,promotion:'q'};else{setPromotion({intent,alt});return}}







 stopCoachSpeech();inputLock.current=true;if(premove){setQueue(q=>q.slice(1));setAutoCommit(true);setPremoveTransition({gameId:s.game.id,from:s.node,to:s.game.nodes.length});}setBusy(true);try{const next=await invoke<Snapshot>(s.node!==s.game.live?'branch_move':'commit_move',{gameId:s.game.id,node:s.node,revision:s.game.revision,uci:intent.from+intent.to+(intent.promotion||''),premove});setPromotion(null);setAnalysis(null);setHint(0);setCoachQuestion('');update(next);sound(moveSound(next),settings);if(mode==='training'){const puzzle=puzzles[training];const success=next.game.nodes[next.node].uci===puzzle.solution||(puzzle.prompt.includes('checkmate')&&!!next.status?.includes('checkmate'));setTrainingMessage(success?'Correct. You found the forcing move!':'Try again. Look for checks and forcing moves.');void invoke('training_progress',{key:puzzle.title,success}).catch(fail);}}







 catch(e){fail(e);setQueue([])}finally{inputLock.current=false;setAutoCommit(false);setBusy(false)}}







 useEffect(()=>{if(!snap||setup||snap.paused||snap.node!==snap.game.live||snap.status||mode!=='play'||snap.game.human==='both')return;if(snap.turn===snap.game.human)return;const key=snap.game.id+':'+snap.game.revision;if(opponentRequest.current===key)return;opponentRequest.current=key;const generation=++opponentGeneration.current;const valid=(gameId:string,revision:number)=>{const s=live.current;return generation===opponentGeneration.current&&s?.game.id===gameId&&s.game.revision===revision&&!s.paused&&!s.status&&!contextRef.current.setup&&contextRef.current.mode==='play'};setEngineStatus('Opponent thinking');if(snap.game.ai_opponent)setProviderTurn({phase:'thinking'});void invoke<{game_id:string;revision:number;uci:string;elapsed_ms?:number}>(snap.game.ai_opponent?'provider_opponent':'opponent',{gameId:snap.game.id,revision:snap.game.revision}).then(async r=>{if(!valid(r.game_id,r.revision))return;if(snap.game.ai_opponent)setProviderTurn({phase:'done',elapsed_ms:r.elapsed_ms});const replyStart=performance.now();if(!snap.game.base_ms){const feedbackKey=snap.game.id+':'+snap.node;while(!feedbackReady.current.has(feedbackKey)&&performance.now()-replyStart<5000){await new Promise(resolve=>setTimeout(resolve,10));if(!valid(r.game_id,r.revision))return;}if(feedbackReady.current.has(feedbackKey)){await new Promise(resolve=>setTimeout(resolve,100));if(!valid(r.game_id,r.revision))return;}while(warningRef.current?.gameId===r.game_id&&warningRef.current.revision===r.revision){await new Promise(resolve=>setTimeout(resolve,30));if(!valid(r.game_id,r.revision))return;}}if(!valid(r.game_id,r.revision))return;const s=live.current!;void invoke<Snapshot>('commit_move',{gameId:r.game_id,revision:r.revision,uci:r.uci,premove:false}).then(async next=>{const current=live.current;if(current?.game.id!==r.game_id||current.game.revision!==r.revision)return;if(s.node!==s.game.live)next=await invoke<Snapshot>('snapshot',{node:s.node});const latest=live.current;if(latest?.game.id!==r.game_id||latest.game.revision!==r.revision)return;update(next);if(contextRef.current.mode==='play')sound(moveSound(next,true),settings)}).catch(e=>{if(generation===opponentGeneration.current)fail(e)});}).catch(e=>{if(generation===opponentGeneration.current){if(snap.game.ai_opponent){setProviderTurn({phase:'error',message:String(e)})}else{opponentRequest.current='';fail(e)}}});return()=>{if(generation===opponentGeneration.current){opponentGeneration.current++;if(snap.game.ai_opponent)void invoke('provider_cancel',{key:snap.game.id});if(opponentRequest.current===key)opponentRequest.current=''}};},[snap?.game.revision,snap?.game.id,setup,mode,snap?.paused,providerRetry]);
 // Drain only when input is actually available. Queue changes and release of
 // the previous commit lock must retry, without cancelling the opponent task.
 useEffect(()=>{if(!snap||setup||busy||inputLock.current||mistakeWarning||snap.paused||snap.status||mode!=='play'||snap.node!==snap.game.live||snap.game.human==='both'||snap.turn!==snap.game.human||!queueRef.current.length)return;const first=queueRef.current[0];const uci=first.from+first.to+(first.promotion||'');if(!snap.legal.includes(uci)){setQueue([]);return;}void commit(first,false,true);},[snap?.game.revision,snap?.game.id,snap?.turn,snap?.paused,setup,mode,busy,mistakeWarning,queue]);







 useEffect(()=>{if(!snap||setup||mode==='review'||mode==='training')return;const analysisLanguage=settings.language;const key=snap.game.id+':'+snap.game.revision+':'+snap.node+':'+mode+':'+analysisLanguage;if(analysisRequest.current===key)return;analysisRequest.current=key;setAnalysis(null);setHint(0);const seq=++sequence.current;setEngineStatus('Checking position…');const analysisBegan=performance.now();void invoke<Analysis>('analyze',{gameId:snap.game.id,revision:snap.game.revision,node:snap.node,sequence:seq,budget:mode==='play'?60:250,multipv:3,fast:mode==='play'}).then(a=>{analysisFinished(analysisBegan);const s=live.current;if(s&&coachLanguage.current===analysisLanguage&&a.game_id===s.game.id&&a.revision===s.game.revision){setEvidence(old=>({...old,[a.game_id+':'+a.node]:a}));setCoachMessages(old=>({...old,[a.game_id+':'+a.node]:a.explanation}));const warningKey=a.game_id+':'+a.node;const mover=s.game.nodes[a.node]?.parent;const humanMove=mover!==null&&mover!==undefined&&s.game.nodes[mover].fen.split(' ')[1]===(s.game.human==='both'?'w':s.game.human);if(contextRef.current.settings.pauseMistakes&&!contextRef.current.settings.quiet&&contextRef.current.mode==='play'&&!contextRef.current.setup&&!s.game.base_ms&&!s.status&&s.node===s.game.live&&a.node===s.node&&a.label==='Blunder'&&humanMove&&!warningSeen.current.has(warningKey)){const reply=a.search.lines[0]?.pv[0];const warning={gameId:a.game_id,revision:a.revision,node:a.node,arrow:reply&&s.legal.includes(reply)?reply:undefined};warningSeen.current.add(warningKey);warningRef.current=warning;setMistakeWarning(warning);}feedbackReady.current.add(warningKey);if(s.node===s.game.live&&a.node===s.game.live&&modeRef.current!=='review'){setCoachMessage(a.explanation);const key=a.game_id+':'+a.node;if(a.node>0&&spoken.current!==key&&!settings.quiet){spoken.current=key;speak(a.explanation,false,true);}}if(a.revision!==s.game.revision||a.node!==s.node||a.sequence!==sequence.current)return;if(a.node>0&&humanMove&&(a.label==="Great"||a.label==="Brilliant")){const gradeKey=a.game_id+":"+a.node;if(!gradeCuePlayed.current.has(gradeKey)){gradeCuePlayed.current.add(gradeKey);playMoveGradeCue(a.label,settings)}}setAnalysis(a);s.game.nodes[a.node].evidence={search:a.search,label:a.label,loss:a.loss,explanation:a.explanation};setSnap({...s});if(a.node>0&&['Blunder','Mistake','Inaccuracy','Miss'].includes(a.label||'')){void invoke<{transcript:string;fallback:boolean}>('narrate',{gameId:a.game_id,revision:a.revision,node:a.node,question:'Review this move'}).then(r=>{const now=live.current;if(!r.fallback&&coachLanguage.current===analysisLanguage&&now?.game.id===a.game_id&&now.game.revision===a.revision&&now.node===a.node&&modeRef.current!=='review'){setCoachMessage(r.transcript);setCoachMessages(old=>({...old,[a.game_id+':'+a.node]:r.transcript}))}}).catch(()=>{});}setEngineStatus(a.search.lines.length?`Stockfish 19 · depth ${a.search.lines[0]?.depth||0} · ${Math.round(a.search.elapsed_ms)} ms`:'Game complete');}}).catch(e=>{if(seq===sequence.current&&!String(e).includes('Stale')){feedbackReady.current.add((live.current?.game.id||'')+':'+live.current?.node);setEngineStatus('Analysis unavailable');fail(e)}})},[snap?.game.revision,snap?.game.id,snap?.node,snap?.paused,setup,mode,settings.language]);







 async function start(root=fen,player=human,difficulty=level,chess960=false){if(startingGame)return;setStartError('');if(opponentMode==='provider'&&player!=='both'&&(!providerReady||!aiSelection)){setStartError(providerBlockReason||(settings.language==='de'?'Schließe zuerst die Einrichtung ab.':'Complete provider setup first.'));if(!document.querySelector<HTMLInputElement>('.provider-consent input')?.checked)document.querySelector<HTMLInputElement>('.provider-consent input')?.focus();return;}setStartingGame(true);try{cancelOpponent();stopCoachSpeech();setShowResult(false);setReviewReport(null);setReviewVisual({});clearWarning();warningSeen.current.clear();feedbackReady.current.clear();const chosen=player==='random'?(Math.random()<.5?'w':'b'):player;const s=await invoke<Snapshot>('new_game',{root:root||null,human:chosen,difficulty,baseMs:time*60000,incrementMs:increment*1000,chess960,aiOpponent:opponentMode==='provider'&&chosen!=='both'?aiSelection:null});setProviderTurn({phase:'idle'});setQueue([]);setPromotion(null);setAnalysis(null);analysisRequest.current='';opponentRequest.current='';setFlip(chosen==='b');update(s);setSetup(false);setModal('');setTrainingMessage('');setCoachQuestion('');const greeting=settings.language==='de'?'Wir verbessern dein Schach beim Spielen. Mach deinen ersten Zug!':'Let’s work on your chess as we play. Make your first move!';setCoachMessage(greeting);sound('start',settings);speak(greeting)}catch(e){setStartError(String(e));fail(e)}finally{setStartingGame(false)}}







 function enterSetup(){cancelOpponent();clearWarning();returnToRecorded();setManualBranches([]);setShowResult(false);setReviewVisual({});setReviewPreview(null);setAutoplay(false);stopCoachSpeech();const s=live.current;setMode('play');setSetup(true);setQueue([]);setPromotion(null);setCoachQuestion('');if(s&&!s.paused&&!s.status)void invoke<Snapshot>('control',{action:'pause',gameId:s.game.id}).then(next=>{if(live.current?.game.id===next.game.id&&live.current.game.revision<=next.game.revision)update(next)}).catch(e=>{if(!String(e).includes('Stale'))fail(e)})}



 async function action(name:string){try{const current=live.current;if(!current)return;cancelOpponent();clearWarning();setQueue([]);sequence.current++;stopCoachSpeech();const s=await invoke<Snapshot>('control',{action:name,gameId:current.game.id});if(live.current?.game.id!==s.game.id)return;update(s);setCoachQuestion('');if(name==='undo'){setAnalysis(null);analysisRequest.current='';sound('move',settings)}}catch(e){fail(e)}}







 async function enterReview(){cancelOpponent();clearWarning();returnToRecorded();const s=live.current;if(!s)return;stopCoachSpeech();setShowResult(false);setAutoplay(false);setQueue([]);setHint(0);setReviewVisual({});sequence.current++;try{const frozen=s.paused?s:await invoke<Snapshot>('control',{action:'pause',gameId:s.game.id});if(live.current?.game.id!==frozen.game.id)return;update(frozen);setSetup(false);setMode('review');}catch(e){fail(e)}}



 async function enterLearning(gameId?:string,theme=''){cancelOpponent();clearWarning();const s=live.current;if(!s)return;returnToRecorded();stopCoachSpeech();setThreatPreview(null);setShowResult(false);setAutoplay(false);setQueue([]);setPromotion(null);sequence.current++;try{const frozen=s.paused||s.status?s:await invoke<Snapshot>('control',{action:'pause',gameId:s.game.id});if(live.current?.game.id!==frozen.game.id)return;update(frozen);setLearningGameId(gameId);setLearningTheme(theme);setLearningKind(gameId||theme?'mistakes':'puzzles');setModal('');setSetup(false);setMode('training')}catch(e){fail(e)}}

 async function reviewStudy(node:number){returnToRecorded();stopCoachSpeech();try{const s=await invoke<Snapshot>('study',{node,practice:false});setMode('analysis');setReviewVisual({});setReviewReport(null);setShowResult(false);update(s);analysisRequest.current='';}catch(e){fail(e)}}



 useEffect(()=>{if(!setup&&mode==='play'&&snap?.status&&snap.node===snap.game.live&&endedGame.current!==snap.game.id){endedGame.current=snap.game.id;setShowResult(true)}},[snap?.status,snap?.game.id,setup,mode]);



 async function view(node:number){if(!snap)return;returnToRecorded();try{stopCoachSpeech();sequence.current++;const s=await invoke<Snapshot>('snapshot',{node});update(s);setQueue([]);setAnalysis(null);setCoachQuestion('');setCoachMessage(evidence[s.game.id+':'+node]?.explanation||s.game.nodes[node]?.evidence?.explanation||'Let’s look at this position.')}catch(e){fail(e)}}







 const path=snap?(()=>{let ids:number[]=[];let i:number|null=snap.game.live;while(i!==null){ids.unshift(i);i=snap.game.nodes[i].parent}return ids})():[];







 const navigate=(delta:number)=>{if(!snap)return;const i=path.indexOf(snap.node);void view(path[Math.max(0,Math.min(path.length-1,i+delta))])};







 useEffect(()=>{const handler=(e:KeyboardEvent)=>{if((e.target as HTMLElement).closest('input,textarea,select,[role=dialog],[role=combobox],[role=listbox]'))return;if(modeRef.current==='review'&&manualRef.current){const l=manualRef.current;if(e.key==='Escape'){returnToRecorded();return;}if(['ArrowLeft','ArrowRight','Home','End'].includes(e.key)){e.preventDefault();if(!inputLock.current)void showManual({...l,cursor:e.key==='Home'?0:e.key==='End'?l.moves.length:Math.max(0,Math.min(l.moves.length,l.cursor+(e.key==='ArrowLeft'?-1:1)))});return;}}if(e.key==='ArrowLeft')navigate(-1);if(e.key==='ArrowRight')navigate(1);if(e.key==='Home'&&path.length)void view(path[0]);if(e.key==='End'&&path.length)void view(path.at(-1)!);if(e.key.toLowerCase()==='f')setFlip(f=>!f);};window.addEventListener('keydown',handler,true);return()=>window.removeEventListener('keydown',handler,true)});







 useEffect(()=>{if(!autoplay||mode==='review')return;const t=setInterval(()=>navigate(1),1500);return()=>clearInterval(t)},[autoplay,snap?.node]);







 useEffect(()=>{if(!snap?.game.base_ms||setup||snap.paused)return;const timer=setInterval(()=>{if(!inputLock.current)void invoke<Snapshot>('snapshot',{node:live.current?.node}).then(s=>{if(s.game.revision===live.current?.game.revision)update(s)}).catch(fail)},200);return()=>clearInterval(timer)},[snap?.game.id,snap?.game.base_ms,snap?.paused,setup]);







 const best=analysis?.search.lines[0];if(evaluationRef.current.gameId!==snap?.game.id)evaluationRef.current={gameId:snap?.game.id||'',line:null};if(best)evaluationRef.current.line=best;const reviewEvaluation=reviewPreview?.search.lines[0]||reviewReport?.moves.find(m=>m.node===snap?.node)?.search.lines[0];const fullEvaluation=(mode==='review'?reviewEvaluation:undefined)||(snap?evidence[snap.game.id+':'+snap.node]?.search.lines[0]||snap.game.nodes[snap.node]?.evidence?.search.lines[0]:undefined)||evaluationRef.current.line;

 const liteDisplay=settings.evaluationProfile==='reference-lite';

 const evaluation=liteDisplay?(referenceEvaluation&&referenceEvaluation.gameId===snap?.game.id?referenceEvaluation.line:undefined):fullEvaluation;

 const evaluationTitle=liteDisplay?(settings.language==='de'?'Stockfish 19 Lite · Vergleichsanzeige. Zugbewertungen und Coach nutzen die vollständige Engine.':'Stockfish 19 Lite comparison display. Move grades and coaching use the full engine.')+' '+referenceStatus:(analysis?'White advantage is positive; full native Stockfish evaluation':'Checking… showing the previous evaluation until the new result arrives');

 const terminalMate=!setup&&(reviewPreview?.snapshot||snap)?.legal.length===0&&!!(reviewPreview?.snapshot||snap)?.check,terminalDraw=!setup&&!reviewPreview&&snap?.node===snap?.game.live&&!!snap?.status?.toLowerCase().includes('draw');const evalText=setup?'0.0':terminalMate?((reviewPreview?.snapshot||snap)!.turn==='b'?'+M0':'−M0'):terminalDraw?'0.0':evaluation?.mate!==null&&evaluation?.mate!==undefined?(evaluation.mate<0?'-M':'M')+Math.abs(evaluation.mate):evaluation?.cp!==null&&evaluation?.cp!==undefined?(evaluation.cp/100).toFixed(1):'…';







 const whitePercent=setup?50:terminalMate?((reviewPreview?.snapshot||snap)!.turn==='b'?100:0):terminalDraw?50:evaluation?.mate?evaluation.mate>0?99:1:Math.max(3,Math.min(97,50+(evaluation?.cp||0)/20));







 async function openLibrary(){setModal('library');try{setArchive(await invoke('library'))}catch(e){fail(e)}}







 async function download(format:string){try{const path=await invoke<string|null>('save_export',{format});if(path)setCoachQuestion('Saved to '+path)}catch(e){fail(e)}}







 function speak(text:string,force=false,queued=false){if(!settings.voice&&!force)return;const de=settings.language==='de';const names:Record<string,string>=de?{K:'König',Q:'Dame',R:'Turm',B:'Läufer',N:'Springer'}:{K:'king',Q:'queen',R:'rook',B:'bishop',N:'knight'};const suffix=(check:string)=>check==='#'?(de?', Schachmatt':', checkmate'):check?(de?', Schach':', check'):'';const readable=text.replace(/\b([KQRBN])([a-h]?[1-8]?)(x?)([a-h][1-8])([+#])?/g,(_,p,_from,capture,to,check)=>`${names[p]} ${capture?(de?'schlägt auf':'takes on'):(de?'nach':'to')} ${to[0]} ${to[1]}${suffix(check)}`).replace(/\b([a-h])x([a-h][1-8])([+#])?/g,(_,file,to,check)=>`${file} ${de?'Bauer schlägt auf':'pawn takes on'} ${to[0]} ${to[1]}${suffix(check)}`).replace(/\b([a-h])([1-8])\b/g,'$1 $2').replaceAll('O-O-O',de?'lange Rochade':'castle queenside').replaceAll('O-O',de?'kurze Rochade':'castle kingside');if(queued)queueCoachSpeech(readable,setVoiceStatus);else void speakCoach(readable,setVoiceStatus,force);}







 async function ask(q:string){const s=live.current;if(!s)return;const token=++questionRequest.current;const language=settings.language;const current=()=>{const n=live.current;return questionRequest.current===token&&coachLanguage.current===language&&n?.game.id===s.game.id&&n.game.revision===s.game.revision&&n.node===s.node};try{const text=await invoke<string>('coach_reply',{gameId:s.game.id,revision:s.game.revision,node:s.node,question:q});if(!current())return;setCoachQuestion(text);speak(text);void invoke<{transcript:string;status:string}>('narrate',{gameId:s.game.id,revision:s.game.revision,node:s.node,question:q}).then(r=>{if(current()&&language===settings.language){setCoachQuestion(r.transcript);setEngineStatus(r.status)}}).catch(()=>{});}catch{if(current())setCoachQuestion(language==='de'?'Die Stellungsprüfung läuft noch. Versuche es gleich noch einmal.':'This position is still being checked. Try again in a moment.')}}







 const ui=(text:string)=>settings.language==='de'?(interfaceDE[text]||text):text;

 const questionRequest=useRef(0),coachLanguage=useRef(settings.language);coachLanguage.current=settings.language;

 if(!snap)return <div className="loading"><Avatar/><h2>Local Chess Coach</h2><p>{error||'Opening your offline chess room…'}</p><p className="muted">Requires the native Windows application. Use npm run tauri dev for development.</p></div>;







 const labelNode=(()=>{if(snap.node!==snap.game.live||snap.game.human==='both')return snap.node;let n=snap.node;while(n>0){const parent=snap.game.nodes[n].parent!;const color=snap.game.nodes[parent].fen.split(' ')[1];if(color===snap.game.human)return n;n=parent;}return 0})();







 const nodeLabel=(id:number)=>evidence[snap.game.id+':'+id]?.label||snap.game.nodes[id]?.evidence?.label;







 const badgeLabel=nodeLabel(labelNode),badgeSquare=snap.game.nodes[labelNode]?.uci.slice(2,4);







 const boardNode=mode==='review'?snap.node:nodeLabel(snap.node)?snap.node:labelNode;const boardLabel=nodeLabel(boardNode),boardSquare=snap.game.nodes[boardNode]?.uci.slice(2,4);



 const boardBadge=boardLabel&&boardSquare&&snap.pieces.some(p=>p.square===boardSquare&&p.color===snap.game.nodes[snap.game.nodes[boardNode].parent!]?.fen.split(' ')[1])?{square:boardSquare,label:boardLabel}:undefined;







 function giveHint(){if(!snap||!analysis||snap.status)return;const next=Math.min(2,hint+1),san=best?.san[0],from=analysis.search.best.slice(0,2),piece=snap.pieces.find(p=>p.square===from);const de=settings.language==='de';const roles:Record<string,string>=de?{p:'Bauern',n:'Springer',b:'Läufer',r:'Turm',q:'Dame',k:'König'}:{p:'pawn',n:'knight',b:'bishop',r:'rook',q:'queen',k:'king'};const idea=san?.includes('#')?(de?'Du kannst hier mattsetzen.':'There is a checkmate here.'):san?.includes('+')?(de?'Prüfe die Schachgebote.':'Look for a checking move.'):san?.includes('x')?(de?'Prüfe die möglichen Schlagzüge.':'Look for a useful capture.'):(de?'Welche Figur kannst du verbessern?':'Which piece could you improve?');const text=next===1?idea+' '+(de?'Schau dir '+(piece?.role==='q'?'deine ':'deinen ')+roles[piece?.role||'p']+' an. Ich habe die Figur markiert.':'Look at your '+roles[piece?.role||'p']+'. I’ve highlighted it for you.'):(de?'Probiere den auf dem Brett markierten Zug.':'Try the move marked on the board.');setHint(next);setCoachQuestion(text);speak(text)}



 const humanColor=snap.game.human==='b'?'Black':'White',opponentColor=humanColor==='White'?'Black':'White';







 const displayedProvider=setup&&opponentMode==='provider'?aiSelection:!setup?snap.game.ai_opponent:null;

 return <div className={'app '+(setup&&opponentMode==='provider'?'provider-play-setup ':'')+(mode==='review'?'review-mode ':'')+(settings.focus?'focus':'')}>







 <nav className="rail" aria-label={ui("Main navigation")}><button className="brand-pawn" title="Local Chess Coach home" onClick={enterSetup}><span className="reference-glyph" aria-hidden="true">{'\u1f2a'}</span></button><button className={setup||mode==='play'?'nav-active':''} aria-current={setup||mode==='play'?'page':undefined} title={ui("Play coach")} onClick={enterSetup}><img src="/nav/play.svg" alt="" aria-hidden="true"/></button><button title={ui("Saved games")} onClick={openLibrary}><span className="reference-glyph" aria-hidden="true">{'\u0398'}</span></button><button className={mode==='review'?'nav-active':''} aria-current={mode==='review'?'page':undefined} title={ui("Game review")} onClick={()=>void enterReview()}><img src="/nav/review.svg" alt="" aria-hidden="true"/></button><button className={mode==='training'?'nav-active':''} aria-current={mode==='training'?'page':undefined} title={ui("Offline training")} onClick={()=>void enterLearning()}><img src="/nav/training.svg" alt="" aria-hidden="true"/></button><button className={mode==='analysis'?'nav-active':''} aria-current={mode==='analysis'?'page':undefined} title={ui("Analysis board")} onClick={()=>{setMode('analysis');setModal('position')}}><img src="/nav/analysis.svg" alt="" aria-hidden="true"/></button><div className="rail-spacer"/><button title={ui("Focus mode")} onClick={()=>changeSettings({focus:!settings.focus})}><Maximize/></button><button title={ui("Mute")} onClick={()=>changeSettings({mute:!settings.mute})}>{settings.mute?<VolumeX/>:<Volume2/>}</button><button title={ui("Settings")} onClick={()=>setModal('settings')}><span className="reference-glyph" aria-hidden="true">{'\u00b7'}</span></button></nav>







 {mode==='training'?<LearningRoom settings={settings} onSettings={changeSettings} onExit={()=>{stopCoachSpeech();setMode('play')}} onSpeak={speak} gameId={learningGameId} initialTheme={learningTheme} initialKind={learningKind}/>:<main className="workspace"><section className="board-room"><header className="player top">{(snap.game.human==='both'||snap.game.human===(flip?'w':'b'))?<div className="you-avatar">{<img src="/player-mark.svg" alt={settings.language==='de'?'Du':'You'}/>}</div>:displayedProvider?<ProviderAvatar provider={displayedProvider.provider}/>:<Avatar/>}<div><strong>{snap.game.human==='both'?(flip?ui('White'):ui('Black')):snap.game.human===(flip?'w':'b')?ui('You'):displayedProvider?providerName(displayedProvider.provider):'Coach David'}</strong><span title="Approximate local strength; ratings are not calibrated">{mode!=='review'&&snap.game.human!=='both'&&snap.game.human!==(flip?'w':'b')&&!displayedProvider?` (${setup?level:snap.game.difficulty})`:''}</span>{displayedProvider&&snap.game.human!==(flip?'w':'b')&&<small className="provider-model">{displayedProvider.display_name} · local CLI</small>}{!setup&&<ReviewMaterial language={settings.language} snap={reviewPreview?.snapshot||snap} color={flip?'w':'b'}/>}</div>{!!snap.game.base_ms&&<div className="clock">{clock(flip?snap.white_ms:snap.black_ms,snap.game.base_ms)}</div>}<button title={settings.language==='de'?'Brett drehen (F)':'Flip board (F)'} onClick={()=>setFlip(!flip)}><FlipVertical/></button></header>







 <div className="board-line">{settings.eval&&<div className={'evaluation '+(flip?'flipped':'')} title={evaluationTitle} data-engine={liteDisplay?'reference-lite':'full'}><div style={{height:whitePercent+'%'}}/><span>{evalText}</span></div>}<Board forceReviewArrows={mode==='review'} reviewArrows={mode==='review'?reviewVisual.arrows||[reviewVisual.arrow].filter((a):a is string=>!!a):[]} reviewBestArrows={mode==='review'?reviewVisual.bestArrows||[]:[]} pendingPremove={autoCommit&&busy&&!setup&&!promotion&&!snap.paused&&!snap.status&&!threatPreview&&mode==='play'} premoveTransition={premoveTransition} interactiveReplay={mode==='review'} inputEpoch={reviewInputEpoch} replay={mode==='review'||snap.node!==snap.game.live} snap={setup?(setupPreview||snap):mode==='review'?reviewPreview?.snapshot||{...snap,game:{...snap.game,human:'both'}}:threatPreview||snap} flip={flip} settings={settings} queue={setup?[]:queue} onMove={(i,a)=>void commit(i,a)} onQueue={setQueue} onQueuePromotion={i=>setPromotion({intent:i,alt:false,premove:true})} badge={setup?undefined:manualLine&&reviewPreview?.analysis?.label?{square:reviewPreview.snapshot.game.nodes[reviewPreview.snapshot.node].uci.slice(2,4),label:reviewPreview.analysis.label}:reviewPreview?undefined:threatPreview?undefined:boardBadge} resultBadges={!setup&&mode==='play'&&snap.node===snap.game.live&&terminalMate?snap.pieces.filter(p=>p.role==='k').map(p=>({square:p.square,kind:p.color===snap.turn?'mate':'winner'})):undefined} coachTarget={mode==='review'?reviewVisual.target:mistakeWarning?.arrow?.slice(2,4)} coachArrowColor={mode==='review'?(manualLine?'#479bea':'#ffb000'):mistakeWarning?'#f04438':undefined} coachFocus={!setup&&!threatPreview&&hint>=1?analysis?.search.best.slice(0,2):undefined} coachArrow={mode==='review'?undefined:mistakeWarning?.arrow||(!setup&&!threatPreview&&hint>=2?analysis?.search.best:undefined)} disabled={!!mistakeWarning||busy||setup||!!threatPreview||!!promotion||(mode==='review'?(reviewPreview?.snapshot||snap).legal.length===0:snap.paused||!!snap.status||(snap.node!==snap.game.live&&snap.game.human!=='both'))}>{threatPreview&&<div className="proof-board-ribbon"><span>{settings.language==='de'?'Berechnete Fortsetzung':'Calculated continuation'}</span><button onClick={()=>setThreatPreview(null)}>{settings.language==='de'?'Zurück zur Partie':'Back to game'}</button></div>}{!setup&&promotion&&<PromotionPicker intent={promotion.intent} color={promotion.premove?snap.game.human:mode==='review'?(reviewPreview?.snapshot||snap).turn:snap.turn} flip={flip} onCancel={()=>setPromotion(null)} onChoose={p=>{if(promotion.premove){setQueue([...queue,{...promotion.intent,promotion:p}]);setPromotion(null)}else void commit({...promotion.intent,promotion:p})}}/>}{showResult&&mode==='play'&&!setup&&<ResultDialog snap={snap} onClose={()=>setShowResult(false)} onReview={()=>void enterReview()} onNew={enterSetup} onShare={()=>{setShowResult(false);setModal('export')}}/>}</Board></div>







 <footer className="player bottom">{(snap.game.human==='both'||snap.game.human===(flip?'b':'w'))?<div className="you-avatar">{<img src="/player-mark.svg" alt={settings.language==='de'?'Du':'You'}/>}</div>:displayedProvider?<ProviderAvatar provider={displayedProvider.provider}/>:<Avatar/>}<div><strong>{snap.game.human==='both'?(flip?ui('Black'):ui('White')):snap.game.human===(flip?'b':'w')?ui('You'):displayedProvider?providerName(displayedProvider.provider):'Coach David'}</strong>{displayedProvider&&snap.game.human!==(flip?'b':'w')&&<small className="provider-model">{displayedProvider.display_name} · local CLI</small>}{!setup&&<ReviewMaterial language={settings.language} snap={reviewPreview?.snapshot||snap} color={flip?'b':'w'}/>}<small>{setup?'':queue.length?`${queue.length}/5 premoves queued · Esc cancels`:snap.node!==snap.game.live?ui('Viewing history · live game continues'):snap.status||ui(snap.turn==='w'?'White to move':'Black to move')}</small></div>{!!snap.game.base_ms&&<div className="clock">{clock(flip?snap.black_ms:snap.white_ms,snap.game.base_ms)}</div>}<button title={ui("Board settings")} onClick={()=>setModal('settings')}><Gear size={18}/></button></footer>







 </section>{mode==='review'&&!setup?<GameReview onTrain={()=>void enterLearning(snap.game.id)} onSavedBranches={saved=>setManualBranches(old=>[...old.filter(l=>!saved.some(s=>s.origin===l.origin&&s.moves.join(' ')===l.moves.join(' '))),...saved])} key={snap.game.id} manualBranches={manualBranches} onResume={(line,cursor)=>void resumeManual(line,cursor)} manualLine={manualLine} manualSnapshot={reviewPreview?.snapshot} manualEvaluation={reviewPreview?.analysis?reviewPreview.search.lines[0]:undefined} manualAnalysis={manualLine?reviewPreview?.analysis:undefined} onManualView={cursor=>{const l=manualRef.current;if(l&&!inputLock.current)void showManual({...l,cursor})}} onReturn={returnToRecorded} snap={snap} settings={settings} onSettings={changeSettings} onView={view} onReport={r=>{setReviewReport(r);setEvidence(old=>({...old,...Object.fromEntries(r.moves.map(m=>[r.game_id+':'+m.node,{...m,game_id:r.game_id,revision:snap.game.revision}]))}));}} onPreview={p=>{if(manualRef.current)return;setReviewInputEpoch(v=>v+1);setReviewPreview(p)}} onVisual={setReviewVisual} onExit={()=>{setReviewPreview(null);setMode('play');setAutoplay(false);void view(snap.game.live)}} onNew={enterSetup} onStudy={n=>void reviewStudy(n)} onExport={()=>setModal('export')} onSpeak={text=>speak(text)}/>:<aside className="coach-panel"><div className="panel-heading">{ui(mode==='review'?'Game review':mode==='training'?'Offline Training':mode==='analysis'?'Analysis':(setup?opponentMode==='provider':!!snap.game.ai_opponent)?'Play vs. AI':'Play vs. Coach')}<CoachVolume language={settings.language} enabled={settings.voice} volume={settings.voiceVolume} onChange={(voice,voiceVolume)=>changeSettings({voice,voiceVolume})}/></div><div className={'coach-intro '+(!setup?'in-game':'')}><Avatar/><div className="coach-feed">{(()=>{const text=setup?(settings.language==='de'?'Ich helfe dir beim Lernen, während wir spielen. Bereit?':'I’ll help you learn strategy while we play. Ready?'):settings.quiet?(settings.language==='de'?'Deine Partie. Frag mich, wenn du einen Hinweis möchtest.':'Your game. Ask for guidance when you want it.'):coachQuestion||snap.status;const feedPath:number[]=[];let feedAt:number|null=snap.node;while(feedAt!==null){feedPath.unshift(feedAt);feedAt=snap.game.nodes[feedAt].parent}const ids=feedPath.filter(n=>n>0&&(evidence[snap.game.id+':'+n]?.explanation||snap.game.nodes[n]?.evidence?.explanation)).slice(mistakeWarning?-1:-2);const messages=text?[{key:'current',text}]:ids.length?ids.map(n=>({key:snap.game.id+':'+n,text:coachMessages[snap.game.id+':'+n]||(settings.language==='de'?'Schauen wir auf deine Möglichkeiten.':'Let’s look at your options.')})): [{key:'greeting',text:coachMessage||(settings.language==='de'?'Lass uns gemeinsam an deinem Schach arbeiten. Mach deinen ersten Zug!':'Let’s work on your chess as we play. Make your first move!')}];return messages.map(m=><div className="speech" key={m.key}><span className="speech-text">{m.text}</span></div>)})()}{mistakeWarning&&<div className="mistake-warning-actions"><button onClick={clearWarning}><span className="reference-glyph warning-continue-icon" aria-hidden="true">{'\u1f29'}</span>{settings.language==='de'?'Weiter':'Continue'}</button><button className="primary" onClick={()=>void action('undo')}><RotateCcw size={16}/>{settings.language==='de'?'Nochmal':'Retry'}</button></div>}</div></div>







 {error&&<div role="alert" className="error">{error}<button title={ui("Dismiss")} onClick={()=>setError('')}><X size={14}/></button></div>}







 {setup?<div className="setup-content"><div className="provider-modes" role="group" aria-label="Opponent mode"><button className={opponentMode==='local'?'chosen':''} onClick={()=>setOpponentMode('local')}>{settings.language==='de'?'Lokaler Coach':'Local coach'}</button><button className={opponentMode==='provider'?'chosen':''} onClick={()=>{setOpponentMode('provider');if(human==='both')setHuman('w')}}>{settings.language==='de'?'KI-Anbieter':'AI provider'}</button></div><div className="setup-row"><strong>{opponentMode==='provider'?(settings.language==='de'?'Deine Farbe':'Your side'):ui(level<=600?'Casual':level<=1600?'Balanced':'Challenging')} <ChevronDown size={14}/></strong><div className="color-choices"><span>{ui("Play As:")}</span>{(opponentMode==='provider'?['w','random','b']:['w','random','b','both']).map(c=><button key={c} className={human===c?'chosen':''} title={ui(({w:'White',b:'Black',random:'Random',both:'Same-PC two player'} as Record<string,string>)[c])} onClick={()=>setHuman(c)}>{c==='both'?<Swords size={22}/>:<span className="reference-glyph">{c==='random'?'\u1fa3':c==='w'?'\u0100':'\u0104'}</span>}</button>)}</div></div>







 {opponentMode==='provider'?<ProviderSetup value={aiSelection} onChange={setAiSelection} ready={providerReady} onReady={(isReady,reason)=>{setProviderReady(isReady);setProviderBlockReason(reason||'');if(isReady)setStartError('')}} language={settings.language}/>:<>{[[400,'Casual','An easier game so you can focus on learning.'],[1200,'Balanced','An even match to play while you learn.'],[2000,'Challenging','A harder game that pushes you to compete and learn.']].map(([l,title,description])=><button key={l} className={'difficulty '+(level===l?'active':'')} onClick={()=>setLevel(Number(l))}><strong>{ui(String(title))}</strong><span>{ui(String(description))}</span></button>)}<button className="show-levels" onClick={()=>setShowLevels(!showLevels)}>{ui("Show All Levels")} <ChevronDown size={14}/></button>{showLevels&&<div className="levels">{[100,400,600,900,1200,1600,2000,2400,2800,3200].map(l=><button className={level===l?'chosen':''} key={l} onClick={()=>setLevel(l)}>{l===3200?ui('Maximum'):l}</button>)}<small>{ui("Local estimates. Beginner profiles are not calibrated ratings.")}</small></div>}</>}







 <details className="game-options"><summary>{ui("Game options")}</summary><div className="setup-options"><label>{ui("Time control")}<select value={time} onChange={e=>setTime(+e.target.value)}><option value={0}>{ui("Untimed learning")}</option><option value={1}>{settings.language==='de'?'1 Minute · Bullet':'1 minute · bullet'}</option><option value={3}>{settings.language==='de'?'3 Minuten · Blitz':'3 minutes · blitz'}</option><option value={5}>{settings.language==='de'?'5 Minuten · Blitz':'5 minutes · blitz'}</option><option value={10}>{settings.language==='de'?'10 Minuten · Schnellschach':'10 minutes · rapid'}</option><option value={30}>{settings.language==='de'?'30 Minuten':'30 minutes'}</option></select></label><label>{ui("Increment (seconds)")}<input type="number" min="0" max="60" value={increment} onChange={e=>setIncrement(+e.target.value)}/></label><label>{ui("Language")}<select value={settings.language} onChange={e=>changeSettings({language:e.target.value})}><option value="en">English</option><option value="de">Deutsch</option></select></label><button onClick={()=>setModal('position')}>{ui("Starting position / FEN")}</button></div></details><div className="offline-note"><span className="status-dot"/> {opponentMode==='provider'?(settings.language==='de'?'Cloud-Gegner / lokaler Coach':'Cloud opponent / local coach'):ui("Offline \u00b7 no account needed")}</div><button className="primary play-button" disabled={startingGame} aria-busy={startingGame} onClick={()=>void start()}><Play size={20} fill="currentColor"/>{startingGame?(settings.language==='de'?'Wird gestartet…':'Starting…'):settings.language==='de'?'Spielen':'Play'}</button>{startError&&<p className="setup-start-error" role="alert">{startError}</p>}</div>







 :<div className="game-content">{snap.game.ai_opponent&&<ProviderTurnPanel selection={snap.game.ai_opponent} turn={providerTurn} onCancel={cancelProviderTurn} onRetry={()=>{opponentRequest.current='';setProviderRetry(n=>n+1)}} onLocal={()=>void switchToLocal(false)} onSkip={()=>void switchToLocal(true)} gameId={snap.game.id} language={settings.language}/>}<OpeningName game={snap.game} node={snap.node} language={settings.language}/>{snap.status&&<div className="result"><strong>{snap.status}</strong><button className="primary" onClick={()=>void enterReview()}>{ui("Review game")}</button><button onClick={enterSetup}>{ui("Rematch / new game")}</button></div>}















 <div className="move-list" ref={moveListRef} aria-label={ui("Move history")}>{path.slice(1).map((n,i)=><React.Fragment key={n}>{i%2===0&&<span className="move-number">{Math.floor(i/2)+1}.</span>}<button className={snap.node===n?'current':''} onClick={()=>void view(n)} aria-label={snap.game.nodes[n].san}>{settings.moveLabels&&nodeLabel(n)&&<MoveBadge language={settings.language} label={nodeLabel(n)!}/>}<MoveSan san={snap.game.nodes[n].san} color={snap.game.nodes[snap.game.nodes[n].parent!].fen.split(' ')[1]}/></button></React.Fragment>)}{path.length===1&&<p className="empty">{ui("Your moves will appear here.")}</p>}</div>{mode==='analysis'&&<div className="variation-list">{snap.game.nodes.filter(n=>n.parent===snap.node).map(n=><button key={n.id} onClick={()=>void view(n.id)}>{n.san}</button>)}<textarea aria-label={ui("Node comment")} key={snap.node} defaultValue={snap.game.nodes[snap.node].comment} placeholder={ui("Comment on this position")} onBlur={e=>void invoke('annotate_node',{node:snap.node,comment:e.target.value}).catch(fail)}/></div>}<div className="history-controls"><button title={ui("First move")} onClick={()=>void view(0)}><ChevronsLeft/></button><button title={ui("Previous move")} onClick={()=>navigate(-1)}><ChevronLeft/></button><button title={ui("Autoplay")} onClick={()=>setAutoplay(!autoplay)}>{autoplay?<Pause/>:<Play/>}</button><button title={ui("Next move")} onClick={()=>navigate(1)}><ChevronRight/></button><button title={ui("Live position")} onClick={()=>void view(snap.game.live)}><ChevronsRight/></button></div>







 {snap.node!==snap.game.live&&<button className="return-live" onClick={()=>void view(snap.game.live)}>{ui("Return to live game")}</button>}







 <details className="engine-details" open={showEngineDetails||mode==='analysis'||mode==='review'}><summary>{ui("Analysis details")}</summary><div className="engine-status"><span className="status-dot"/>{engineStatus}</div>{badgeLabel&&!settings.quiet&&<div className={'classification '+badgeLabel.toLowerCase()}><MoveBadge language={settings.language} label={badgeLabel}/><strong>{badgeLabel}</strong><span>Provisional · local human feedback</span></div>}{best&&<div className="lines">{analysis?.search.lines.map((l,i)=><div key={i}><span>{l.mate!==null?`M${l.mate}`:((l.cp||0)/100).toFixed(2)}</span>{l.san.slice(0,6).join(' ')}<small>d{l.depth} · {(l.nps/1000000).toFixed(1)}M nps</small></div>)}</div>}{analysis?.detail&&<p className="analysis-proof">{analysis.detail}</p>}</details>







 {hint>0&&mode!=='play'&&<div className="hint">{hint===1?`${best?.san[0]?.includes('#')?'There is a checkmate in one.':best?.san[0]?.includes('+')?'Look for a checking move.':best?.san[0]?.includes('x')?'Look for a useful capture.':'Look at your piece activity.'} Consider the ${({p:'pawn',n:'knight',b:'bishop',r:'rook',q:'queen',k:'king'} as Record<string,string>)[snap.pieces.find(p=>p.square===analysis?.search.best.slice(0,2))?.role||'p']} on ${analysis?.search.best.slice(0,2)||'the best square'}.`:`Best move: ${best?.san[0]||'Checking…'}`}</div>}







 {mode==='training'&&<div className="hint">{trainingMessage||puzzles[training].prompt}<button onClick={()=>void start(puzzles[training].fen,'both',400)}>Retry exercise</button><button onClick={()=>setModal('training')}>Choose practice</button></div>}







 <WeaknessNudge language={settings.language} gameId={snap.game.id} hintTrigger={hint} onPractice={theme=>void enterLearning(undefined,theme)}/><ThreatLesson snap={snap} language={settings.language} previewActive={!!threatPreview} onPreview={setThreatPreview}/><details className="coach-question-menu"><summary>{ui("Ask your coach")}</summary><div className="coach-questions"><button onClick={()=>ask('Why was that a mistake?')}>{ui("Why?")}</button><button onClick={()=>ask('What is the threat?')}>{ui("Threat?")}</button><button onClick={()=>ask('What should my plan be?')}>{ui("Plan?")}</button><button onClick={()=>{setHint(2);setShowEngineDetails(true);setCoachQuestion(settings.language==='de'?'Ich habe die Idee auf dem Brett markiert. Die genaue Fortsetzung findest du in den Analysedetails.':'I’ve marked the idea on the board. You’ll find the exact continuation in the analysis details.')}}>{ui("Show line")}</button><button onClick={()=>{void invoke<Snapshot>('study',{node:snap.node,practice:false}).then(s=>{update(s);setMode('analysis')}).catch(fail)}}>{ui("Explore / retry")}</button></div></details>







 <div className="actions"><button title={ui("Resign")} disabled={!!snap.status} onClick={()=>setModal('resign')}><Flag size={20}/><span>{ui("Resign")}</span></button><button className="primary" disabled={!analysis||!!snap.status||!!threatPreview} onClick={giveHint}><Lightbulb size={19}/>{ui('Hint')} {hint?`(${hint}/2)`:''}</button><button onClick={()=>void action('undo')} disabled={!snap.game.live}><RotateCcw size={18}/>{ui("Takeback")}</button></div><div className="secondary-actions"><button title={ui(snap.paused?'Resume':'Pause')} aria-label={ui(snap.paused?'Resume':'Pause')} disabled={!!snap.status} onClick={()=>void action(snap.paused?'resume':'pause')}>{snap.paused?<Play size={16}/>:<Pause size={16}/>}</button><button title={ui("Offer draw")} aria-label={ui("Offer draw")} disabled={!!snap.status} onClick={()=>setModal('draw')}>½</button><button title={ui("Export game")} aria-label={ui("Export game")} onClick={()=>setModal('export')}><Download size={18}/></button><button title={ui("Coach settings")} onClick={()=>setModal('settings')}><Gear size={18}/></button></div></div>}







 </aside>}</main>}















 {modal&&<div className="modal-backdrop" onClick={()=>setModal('')}><div role="dialog" aria-modal="true" aria-label={modal} className={'modal '+(modal==='settings'?'settings-modal':'')} onClick={e=>e.stopPropagation()}><button className="close" title={ui("Close dialog")} onClick={()=>setModal('')}><X/></button>







 {modal==='settings'&&<SettingsPanel settings={settings} onChange={changeSettings} onPreview={()=>speak(settings.language==='de'?'Ich bin bereit. Lass uns zusammen Schach spielen!':'Hi, I’m your coach. Let’s find a good move together.',true)} voiceStatus={voiceStatus} diagnostics={diagnostics} onMeasure={()=>{void invoke('diagnostics').then(d=>setDiagnostics(JSON.stringify({native:d,ui:metrics(),speech:speechMetrics()},null,2))).catch(fail)}} onBenchmark={()=>{setDiagnostics('Benchmark running…');void invoke('benchmark').then(d=>setDiagnostics(JSON.stringify(d,null,2))).catch(fail)}} onExport={()=>{const url=URL.createObjectURL(new Blob([diagnostics],{type:'application/json'}));const a=document.createElement('a');a.href=url;a.download='coach-diagnostics-'+Date.now()+'.json';a.click();URL.revokeObjectURL(url)}}/>}







 {modal==='position'&&<><h2>{ui("Position editor")}</h2><PositionEditor initial={fen||snap.fen} onStart={(f,c)=>void start(f,mode==='analysis'?'both':human,level,c)} onError={setError}/><label>FEN<textarea value={fen} onChange={e=>setFen(e.target.value)} placeholder="Leave empty for the initial position"/></label><div className="opening-buttons">{openings.map(o=><button key={o.name} onClick={()=>setFen(o.fen)}>{o.name}</button>)}</div><p className="muted">The native rules library validates the position before play.</p><button className="primary" onClick={()=>void start(fen,mode==='analysis'?'both':human)}>{ui("Start from position")}</button></>}







 {modal==='library'&&<><h2>{ui("Saved games")}</h2><input placeholder={ui("Search games")} value={filter} onChange={e=>setFilter(e.target.value)}/><div className="archive">{archive.filter(g=>g.name.toLowerCase().includes(filter.toLowerCase())).map(g=><button key={g.id} onClick={()=>{void invoke<Snapshot>('load_game',{id:g.id}).then(s=>{update(s);setSetup(false);setModal('');setMode('play');analysisRequest.current='';opponentRequest.current=''}).catch(fail)}}><strong>{g.name}</strong><span>{g.moves} plies · {g.result||'In progress'}</span></button>)}</div><button onClick={()=>setModal('import')}><Upload size={16}/>Import PGN / restore JSON</button></>}







 {modal==='export'&&<><h2>{ui("Export game")}</h2><button className="primary" onClick={()=>void download('json')}>JSON backup · entire tree</button><button onClick={()=>void download('pgn')}>PGN · variations and comments</button><button onClick={()=>{void navigator.clipboard.writeText(snap.fen).then(()=>setCoachQuestion('FEN copied.')).catch(fail)}}>{ui("Copy FEN")}</button><p className="muted">PGN keeps variations, comments and NAGs. JSON also keeps clock snapshots and evidence. Credentials are never included.</p></>}







 {modal==='import'&&<><h2>{ui("Restore backup")}</h2><textarea value={importText} onChange={e=>setImportText(e.target.value)} placeholder="Paste PGN (multiple games accepted) or a Local Chess Coach JSON backup"/><button onClick={()=>{void invoke<Snapshot>(importText.trim().startsWith('{')?'import_backup':'import_pgn',{data:importText}).then(s=>{update(s);setSetup(false);setModal('')}).catch(fail)}}>Validate and restore as a new game</button></>}







 {modal==='training'&&<><h2>Offline practice</h2><p>Original introductory exercises · {puzzles.length} positions</p>{puzzles.map((p,i)=><button className="training-choice" key={p.title} onClick={()=>{setTraining(i);setFen(p.fen);void start(p.fen,'both',400);}}><strong>{p.title}</strong><span>{p.prompt}</span></button>)}</>}







 {(modal==='resign'||modal==='draw')&&<><h2>{ui(modal==='resign'?'Resign this game?':'Agree to a draw?')}</h2><button className="primary" onClick={()=>{void action(modal);setModal('')}}>{ui("Confirm")}</button><button onClick={()=>setModal('')}>{ui("Keep playing")}</button></>}







 </div></div>}







 </div>







}







const openings=[{name:'Initial position',fen:''},{name:'Italian Game',fen:'r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3'},{name:'Queen’s Gambit',fen:'rnbqkbnr/ppp1pppp/8/3p4/2PP4/8/PP2PPPP/RNBQKBNR b KQkq c3 0 2'}];







const puzzles=[{title:'Back-rank finish',prompt:'White to move. Find checkmate in one.',fen:'6k1/5ppp/8/8/8/8/5PPP/4R1K1 w - - 0 1',solution:'e1e8'},{title:'Queen and king',prompt:'White to move. Deliver checkmate.',fen:'7k/8/5KQ1/8/8/8/8/8 w - - 0 1',solution:'g6g7'},{title:'Promotion race',prompt:'White to move. Promote with check.',fen:'7k/4P3/6K1/8/8/8/8/8 w - - 0 1',solution:'e7e8q'}];







createRoot(document.getElementById('root')!).render(<App/>);





















