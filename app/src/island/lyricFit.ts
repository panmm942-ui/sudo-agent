export type FitLine={main:string;sub?:string};

// Measuring forces several synchronous layouts of a hidden probe. Results depend only
// on the text, the box and the typography, so each combination is measured once.
const cache=new Map<string,number>();
const CACHE_LIMIT=400;

function metrics(container:HTMLElement){
 const style=getComputedStyle(container);
 const width=container.clientWidth-(parseFloat(style.paddingLeft)||0)-(parseFloat(style.paddingRight)||0);
 return {style,width,height:container.clientHeight,
  gap:style.getPropertyValue('--lyric-gap').trim()||'4px',
  font:style.getPropertyValue('--lyric-font').trim()||style.fontFamily,
  weight:style.getPropertyValue('--lyric-weight').trim()||'600',
  leading:style.getPropertyValue('--lyric-leading').trim()||'1.16'};
}

/** Largest font size (px) at which `lines` fit `container`; cached. Does not change the container. */
export function measureLyrics(container:HTMLElement,lines:FitLine[],spotlight=true,maxFont=42):number|null {
 if(!container.clientWidth||!container.clientHeight)return null;
 const m=metrics(container);
 const key=JSON.stringify([m.width,m.height,m.gap,m.font,m.weight,m.leading,spotlight,maxFont,lines]);
 const known=cache.get(key);
 if(known!==undefined){cache.delete(key);cache.set(key,known);return known;}
 const probe=document.createElement('div');
 Object.assign(probe.style,{position:'fixed',left:'-10000px',top:'0',visibility:'hidden',pointerEvents:'none',width:`${m.width}px`,display:'flex',flexDirection:'column',gap:m.gap,fontFamily:m.font,whiteSpace:'pre-wrap',overflowWrap:'anywhere',contain:'layout style'});
 const mains:HTMLParagraphElement[]=[],subs:HTMLParagraphElement[]=[];
 for(const row of lines){const block=document.createElement('div'),p=document.createElement('p');p.textContent=row.main.trim()?row.main:'';Object.assign(p.style,{margin:'0',fontWeight:m.weight,lineHeight:m.leading});block.append(p);mains.push(p);
  if(row.sub){const sub=document.createElement('p');sub.textContent=row.sub;Object.assign(sub.style,{margin:'0',fontWeight:'450',lineHeight:'1.2'});subs.push(sub);block.append(sub);}probe.append(block);
 }
 document.body.append(probe);
 const fits=(size:number)=>{mains.forEach((p,i)=>p.style.fontSize=`${size*(spotlight&&i!==1?.82:1)}px`);subs.forEach(p=>p.style.fontSize=`${Math.max(10,size*.45)}px`);return probe.getBoundingClientRect().height<=m.height-2&&probe.scrollWidth<=m.width;};
 let low=8,high=maxFont;
 for(let i=0;i<9;i++){const mid=(low+high)/2;if(fits(mid))low=mid;else high=mid;}
 probe.remove();
 const size=Math.floor(low*10)/10;
 cache.set(key,size);
 if(cache.size>CACHE_LIMIT)cache.delete(cache.keys().next().value!);
 return size;
}

/** Measure the final rows, independent of transient outgoing animation nodes, and apply the size. */
export function fitLyrics(container:HTMLElement,lines:FitLine[],spotlight=true,maxFont=42):void {
 const size=measureLyrics(container,lines,spotlight,maxFont);
 if(size===null)return;
 const value=`${size}px`;
 if(container.style.getPropertyValue('--lyric-size')!==value)container.style.setProperty('--lyric-size',value);
}
