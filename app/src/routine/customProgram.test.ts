import {expect,it} from 'vitest';
import {occurrencesFor,editableProgram} from './program';
import {DEFAULTS,loadPreferences} from '../island/preferences';
it('runs custom weekly reminders beyond the personal routine duration, with stable ids and selected days',()=>{
  const program=[{id:'mine',label:'My reminder',detail:'My details',time:'08:30',days:[1],enabled:true}];
  const result=occurrencesFor(new Date(2027,2,1),new Date(2026,10,2),{...DEFAULTS,program});
  expect(result).toMatchObject([{id:'routine:2027-03-01:custom-mine',time:'08:30',label:'My reminder',detail:'My details'}]);
  expect(occurrencesFor(new Date(2027,2,2),new Date(2026,10,2),{...DEFAULTS,program})).toEqual([]);
  expect(occurrencesFor(new Date(2027,2,1),new Date(2026,10,2),{...DEFAULTS,program:[]})).toEqual([]);
});
it('starts empty with no built-in schedule and validates saved custom entries',()=>{
  expect(editableProgram(DEFAULTS)).toEqual([]);
  expect(occurrencesFor(new Date(2026,10,2),new Date(2026,10,2),DEFAULTS)).toEqual([]);
  expect(occurrencesFor(new Date(2026,10,2),new Date(2026,10,2))).toEqual([]);
  const loaded=loadPreferences({getItem:()=>JSON.stringify({program:[{id:'a',label:'Hello',detail:'',time:'08:00',days:[1,1,8],enabled:true},{id:'bad',time:'99:00'}]})});
  expect(loaded.program).toEqual([{id:'a',label:'Hello',detail:'',time:'08:00',days:[1],enabled:true}]);
  expect(editableProgram(loaded)).toEqual(loaded.program);
});
