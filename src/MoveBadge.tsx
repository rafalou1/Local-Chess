const files:Record<string,string>={Best:'best',Excellent:'excellent',Good:'good',Inaccuracy:'inaccuracy',Mistake:'mistake',Blunder:'blunder',Brilliant:'brilliant',Great:'great',Book:'book',Miss:'miss'};
export function MoveBadge({label,className='',style,language='en'}:{label:string;language?:string;className?:string;style?:React.CSSProperties}) {
  const file=files[label];
  return file ? <img className={`move-badge ${className}`} src={`/badges/${file}.svg`} alt={label} title={language==='de'?({Best:'Am besten',Excellent:'Exzellent',Good:'Gut',Great:'Großartig',Brilliant:'Brillant',Book:'Theorie',Inaccuracy:'Ungenau',Mistake:'Fehler',Blunder:'Patzer',Miss:'Verpasst'} as Record<string,string>)[label]:label} style={style} draggable={false}/> : null;
}
