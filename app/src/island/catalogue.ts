import {validateTab,type TabPackage} from './extensions';
export const DEFAULT_REPOSITORY='panmm942-ui/sudo-agent';
export function repositoryName(value:string):string {
 if(!/^[A-Za-z0-9][A-Za-z0-9-]{0,38}\/[A-Za-z0-9_.-]{1,100}$/.test(value)||value.split('/').some(x=>x==='.'||x==='..'))throw Error('Enter a GitHub repository as owner/name.');
 return value;
}
export type PublicTab={tab:TabPackage;url:string;author:string};
export function publicationBody(tab:TabPackage):string{return `Sudo tab package v${tab.version}\n\n\`\`\`json\n`+JSON.stringify(validateTab(tab),null,2)+'\n```';}
export function parsePublication(issue:unknown,repo:string):PublicTab|null {
 if(!issue||typeof issue!=='object')return null;
 const i=issue as Record<string,unknown>;
 if(i.pull_request||typeof i.body!=='string'||i.body.length>125000||typeof i.html_url!=='string'||!i.html_url.startsWith(`https://github.com/${repositoryName(repo)}/issues/`))return null;
 if(!/^\d+$/.test(i.html_url.split('/').at(-1)??''))return null;
 const body=i.body.replace(/\r\n/g,'\n');
 const marker=/Sudo tab package v[12]\n\n```json\n/.exec(body),end=body.lastIndexOf('\n```');
 if(!marker||end<marker.index+marker[0].length)return null;
 try{return {tab:validateTab(JSON.parse(body.slice(marker.index+marker[0].length,end))),url:i.html_url,author:typeof (i.user as {login?:unknown})?.login==='string'?(i.user as {login:string}).login:'GitHub contributor'};}catch{return null;}
}
export async function checkPublicRepository(repo:string,signal:AbortSignal,get:typeof fetch=fetch):Promise<void>{
 const r=await get(`https://api.github.com/repos/${repositoryName(repo)}`,{signal:AbortSignal.any([signal,AbortSignal.timeout(10000)]),headers:{Accept:'application/vnd.github+json'}});
 if(r.status===404)throw Error('This repository is not publicly accessible. Choose a public repository with Issues enabled. Local tabs stay private.');
 if(!r.ok)throw Error('GitHub is unavailable or its request limit was reached. Try later.');
 const data=await r.json();
 if(data.private!==false||data.has_issues!==true)throw Error('Public sharing requires a public repository with Issues enabled.');
}
export async function readCatalogue(repo:string,signal:AbortSignal,get:typeof fetch=fetch):Promise<PublicTab[]>{
 await checkPublicRepository(repo,signal,get);
 const r=await get(`https://api.github.com/repos/${repositoryName(repo)}/issues?state=open&per_page=100`,{signal:AbortSignal.any([signal,AbortSignal.timeout(10000)]),headers:{Accept:'application/vnd.github+json'}});
 if(!r.ok)throw Error('Could not load the public catalogue. Try later.');
 const data=await r.json();if(!Array.isArray(data))throw Error('GitHub returned an invalid catalogue.');
 return data.slice(0,100).flatMap(issue=>{const entry=parsePublication(issue,repo);return entry?[entry]:[];});
}
