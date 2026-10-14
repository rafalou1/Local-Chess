import {useId,useLayoutEffect,useRef,useState} from 'react';
import {createPortal} from 'react-dom';
import {Check,ChevronDown} from 'lucide-react';
import './SelectField.css';
export function SelectField({label,value,options,disabled=false,onChange}:{label:string;value:string;options:{value:string;label:string}[];disabled?:boolean;onChange:(value:string)=>void}){
 const id=useId(),button=useRef<HTMLButtonElement>(null),menu=useRef<HTMLDivElement>(null),typed=useRef({text:'',time:0});
 const [open,setOpen]=useState(false),[active,setActive]=useState(0),[position,setPosition]=useState({left:0,top:0,width:0,maxHeight:260});
 const selected=Math.max(0,options.findIndex(o=>o.value===value));
 const show=()=>{setActive(selected);setOpen(true)};
 const choose=(index:number)=>{if(options[index])onChange(options[index].value);setOpen(false);button.current?.focus()};
 useLayoutEffect(()=>{if(!open)return;const anchor=button.current;if(!anchor)return;
  const place=()=>{const r=anchor.getBoundingClientRect(),spaceBelow=innerHeight-r.bottom-12,spaceAbove=r.top-12,height=Math.min(260,options.length*42+12);const below=spaceBelow>=Math.min(height,160)||spaceBelow>=spaceAbove;const maxHeight=Math.max(84,Math.min(260,below?spaceBelow:spaceAbove));setPosition({left:Math.max(8,Math.min(r.left,innerWidth-r.width-8)),top:below?r.bottom+6:r.top-6-Math.min(height,maxHeight),width:r.width,maxHeight})};
  place();const outside=(e:PointerEvent)=>{if(!anchor.contains(e.target as Node)&&!menu.current?.contains(e.target as Node))setOpen(false)};
  const blur=()=>setOpen(false);document.addEventListener('pointerdown',outside);window.addEventListener('resize',place);window.addEventListener('scroll',place,true);window.addEventListener('blur',blur);
  return()=>{document.removeEventListener('pointerdown',outside);window.removeEventListener('resize',place);window.removeEventListener('scroll',place,true);window.removeEventListener('blur',blur)};
 },[open,options.length]);
 useLayoutEffect(()=>{if(open)menu.current?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({block:'nearest'})},[active,open]);
 useLayoutEffect(()=>{if(disabled)setOpen(false)},[disabled]);
 return <><button ref={button} type="button" role="combobox" aria-label={label} aria-expanded={open} aria-haspopup="listbox" aria-controls={open?id:undefined} aria-activedescendant={open?`${id}-${active}`:undefined} disabled={disabled} className={'select-field'+(open?' open':'')} onClick={()=>open?setOpen(false):show()} onKeyDown={e=>{
  if(['ArrowDown','ArrowUp','Home','End','Enter',' ','Escape'].includes(e.key)){e.preventDefault();e.stopPropagation()}
  if(e.key==='Escape'){setOpen(false);return}if(e.key==='Tab'){setOpen(false);return}
  if(e.key==='Enter'||e.key===' '){open?choose(active):show();return}
  if(e.key==='ArrowDown'||e.key==='ArrowUp'){if(!open){show();return}setActive(i=>Math.max(0,Math.min(options.length-1,i+(e.key==='ArrowDown'?1:-1))));return}
  if(e.key==='Home'||e.key==='End'){setOpen(true);setActive(e.key==='Home'?0:options.length-1);return}
  if(e.key.length===1&&!e.ctrlKey&&!e.metaKey&&!e.altKey){const now=performance.now();typed.current={text:(now-typed.current.time<700?typed.current.text:'')+e.key.toLowerCase(),time:now};const index=options.findIndex(o=>o.label.toLowerCase().startsWith(typed.current.text));if(index>=0){setActive(index);setOpen(true)}e.stopPropagation()}
 }}><span>{options[selected]?.label||value}</span><ChevronDown size={16}/></button>{open&&createPortal(<div id={id} ref={menu} role="listbox" aria-label={label} className="select-field-menu" style={position}>{options.map((o,i)=><div key={o.value} id={`${id}-${i}`} role="option" aria-selected={o.value===value} data-index={i} className={'select-field-option'+(i===active?' active':'')+(o.value===value?' selected':'')} onPointerDown={e=>e.preventDefault()} onPointerMove={()=>setActive(i)} onClick={()=>choose(i)}><span>{o.label}</span>{o.value===value&&<Check size={15}/>}</div>)}</div>,document.body)}</>
}
