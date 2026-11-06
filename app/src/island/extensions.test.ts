import {expect,it} from 'vitest';
import {validateTab,EXAMPLE_TAB,MAX_TAB_BYTES,TAB_PROMPT} from './extensions';
it('rejects generated JavaScript syntax errors before installation',()=>{
 expect(()=>validateTab({...EXAMPLE_TAB,js:'const broken = ;'})).toThrow('JavaScript syntax');
});
it('retains version 1 imports and round-trips executable version 2 without secrets',()=>{
 const legacy={format:'sudo-tab',version:1,id:'old',name:'Old',compact:'Hello',blocks:[{type:'text',text:'<b>Literal</b>'},{type:'clock'},{type:'focus'}]};
 expect(validateTab(legacy)).toEqual(legacy);
 expect(validateTab(JSON.parse(JSON.stringify(EXAMPLE_TAB)))).toEqual(EXAMPLE_TAB);
 const connected={...EXAMPLE_TAB,permissions:{network:['https://gmail.googleapis.com']},connections:[{id:'gmail',name:'Gmail token',origin:'https://gmail.googleapis.com',header:'Authorization',prefix:'Bearer '}]};
 expect(validateTab(connected)).toEqual(connected);
 expect(()=>validateTab({...connected,connections:[{...connected.connections[0],secret:'never-export'}]})).toThrow();
 expect(TAB_PROMPT).toContain('Windows');
});
it('rejects invalid icons, HTML executable entrypoints, remote resources and unsupported shapes',()=>{
 for(const change of [{script:'bad()'},{id:'../file'},{icon:'unknown'},{version:3},{html:'<script>alert(1)</script>'},{html:'<button onclick="bad()">bad</button>'},{html:'<img src="https://api.example.com">'},{html:'<iframe></iframe>'},{html:'<a href="https://example.com">Go</a>'},{css:'a'.repeat(20001)},{js:'a'.repeat(60001)}])expect(()=>validateTab({...EXAMPLE_TAB,...change})).toThrow();
 expect(()=>validateTab({...EXAMPLE_TAB,html:'a'.repeat(MAX_TAB_BYTES)})).toThrow();
});
it('permits only exact public HTTPS origins and valid connection declarations',()=>{
 for(const origin of ['http://api.example.com','https://api.example.com/','https://api.example.com/path','https://user@api.example.com','https://127.0.0.1','https://localhost','https://a.local','https://*.example.com'])expect(()=>validateTab({...EXAMPLE_TAB,permissions:{network:[origin]}})).toThrow();
 expect(()=>validateTab({...EXAMPLE_TAB,connections:[{id:'a',name:'A',origin:'https://api.example.com',header:'Authorization',prefix:'Bearer '}]})).toThrow();
 expect(()=>validateTab({...EXAMPLE_TAB,permissions:{network:['https://api.example.com','https://api.example.com']}})).toThrow();
});
