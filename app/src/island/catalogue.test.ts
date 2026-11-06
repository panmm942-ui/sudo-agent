import {expect,it,vi} from 'vitest';import {EXAMPLE_TAB} from './extensions';import {repositoryName,publicationBody,parsePublication,checkPublicRepository} from './catalogue';
it('keeps packages local until publication and validates public catalogue entries',()=>{
 const issue={body:publicationBody(EXAMPLE_TAB),html_url:'https://github.com/owner/catalogue/issues/1',user:{login:'owner'}};
 expect(parsePublication(issue,'owner/catalogue')?.tab).toEqual(EXAMPLE_TAB);
 expect(parsePublication({...issue,body:issue.body.replace(/\n/g,'\r\n')},'owner/catalogue')?.tab).toEqual(EXAMPLE_TAB);
 const withFence={format:'sudo-tab' as const,version:1 as const,id:'legacy',name:'Legacy',compact:'Example',blocks:[{type:'text' as const,text:'An example with ``` inside text'}]};
 expect(parsePublication({...issue,body:publicationBody(withFence)},'owner/catalogue')?.tab).toEqual(withFence);
 expect(parsePublication({...issue,html_url:'https://evil.example/1'},'owner/catalogue')).toBeNull();
 expect(parsePublication({...issue,body:'Sudo tab package v1\n```json\n{"script":"bad"}\n```'},'owner/catalogue')).toBeNull();
 expect(()=>repositoryName('../secret')).toThrow();
});
it('refuses private and inaccessible repositories before opening publication',async()=>{
 await expect(checkPublicRepository('owner/catalogue',new AbortController().signal,vi.fn().mockResolvedValue({ok:true,json:async()=>({private:true,has_issues:true})}))).rejects.toThrow('public repository');
 await expect(checkPublicRepository('owner/catalogue',new AbortController().signal,vi.fn().mockResolvedValue({status:404,ok:false}))).rejects.toThrow('not publicly accessible');
});
