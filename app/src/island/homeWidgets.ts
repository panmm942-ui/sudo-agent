export const HOME_WIDGETS = [
  {id:'clock',label:'Clock & date'}, {id:'greeting',label:'Greeting'}, {id:'reminders',label:'Upcoming reminder'},
  {id:'focus',label:'Focus timer'}, {id:'note',label:'Personal note'}, {id:'weather',label:'Weather'},
  {id:'battery',label:'Battery'},
] as const;
export type HomeWidget = typeof HOME_WIDGETS[number]['id'];
export type FocusTimer = {remaining:number;deadline:number|null};
export const FOCUS_KEY='sudo.home.focus.v1';
export const freshFocus = (minutes=25):FocusTimer => ({remaining:minutes*60_000,deadline:null});
export function focusRemaining(timer:FocusTimer,now:number):number {
  return Math.max(0,timer.deadline===null ? timer.remaining : timer.deadline-now);
}
export function startFocus(timer:FocusTimer,now:number,minutes=25):FocusTimer {
  const remaining=focusRemaining(timer,now)||minutes*60_000;
  return {remaining,deadline:now+remaining};
}
export function pauseFocus(timer:FocusTimer,now:number):FocusTimer {
  return {remaining:focusRemaining(timer,now),deadline:null};
}
export function loadFocus(raw:string|null):FocusTimer {
  try {
    const v=JSON.parse(raw??'null');
    if(v && Number.isFinite(v.remaining) && v.remaining>=0 && v.remaining<=86_400_000 && (v.deadline===null || (Number.isFinite(v.deadline)&&v.deadline>0))) return {remaining:v.remaining,deadline:v.deadline};
  } catch { /* Older or invalid storage uses a fresh timer. */ }
  return freshFocus();
}
