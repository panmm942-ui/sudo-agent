export const BORDER_STYLES = [
  {id:'glass',label:'Liquid glass',description:'Original refractive rim'},
  {id:'white-line',label:'White outline',description:'A fine white contour over glass'},
  {id:'pearl',label:'Pearl edge',description:'A soft, satin highlight'},
  {id:'graphite',label:'Graphite edge',description:'A quiet smoked contour'},
  {id:'none',label:'Borderless glass',description:'A clear lens with a restrained edge'},
] as const;
export type IslandBorder = typeof BORDER_STYLES[number]['id'];
export function borderIndex(border:IslandBorder|boolean):number{return border===true?1:border===false?0:Math.max(0,BORDER_STYLES.findIndex(b=>b.id===border));}
