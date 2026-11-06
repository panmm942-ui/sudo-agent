import {version as currentVersion} from '../../package.json';
import {invoke,isTauri} from '@tauri-apps/api/core';
export {currentVersion};
export const RELEASES_URL='https://github.com/panmm942-ui/sudo-agent/releases';
export type Release={version:string;url:string;downloadable:boolean};
export type DownloadedUpdate={version:string;path:string};
export function newerVersion(candidate:string,current:string):boolean {
  const parse=(v:string)=>/^v?(\d+)\.(\d+)\.(\d+)$/.exec(v)?.slice(1).map(Number);
  const a=parse(candidate),b=parse(current);
  if(!a||!b)return false;
  for(let i=0;i<3;i++){if(a[i]!==b[i])return a[i]>b[i];}
  return false;
}
export function parseRelease(raw:unknown):Release|null {
  if(!raw||typeof raw!=='object')return null;
  const r=raw as Record<string,unknown>;
  if(r.draft||r.prerelease||typeof r.tag_name!=='string'||!/^v?\d+\.\d+\.\d+$/.test(r.tag_name)||typeof r.html_url!=='string'||!r.html_url.startsWith(RELEASES_URL+'/tag/'))return null;
  const name=`sudo-island_${r.tag_name.replace(/^v/,'')}_x64-setup.exe`;
  const downloadable=Array.isArray(r.assets)&&r.assets.some((asset:Record<string,unknown>)=>asset?.name===name&&asset.state==='uploaded'&&typeof asset.id==='number'&&asset.id>0&&typeof asset.size==='number'&&asset.size>0&&asset.size<=200*1024*1024&&typeof asset.digest==='string'&&/^sha256:[a-f0-9]{64}$/i.test(asset.digest));
  return {version:r.tag_name,url:r.html_url,downloadable};
}
export async function latestRelease():Promise<Release> {
  if(isTauri()) {
    try {
      const release=parseRelease(await invoke('check_github_release'));
      if(!release)throw new Error('GitHub did not return a supported stable release.');
      return release;
    } catch(error) {throw error instanceof Error?error:new Error(String(error));}
  }
  const response=await fetch('https://api.github.com/repos/panmm942-ui/sudo-agent/releases/latest',{headers:{Accept:'application/vnd.github+json'},signal:AbortSignal.timeout(10_000)});
  if(response.status===404)throw new Error('No public release is accessible yet. The repository may be private or have no published release.');
  if(response.status===403||response.status===429)throw new Error('GitHub’s request limit was reached. Please try later.');
  if(!response.ok)throw new Error('GitHub could not check for updates. Please try later.');
  const release=parseRelease(await response.json());
  if(!release)throw new Error('GitHub did not return a supported stable release.');
  return release;
}
export async function checkRelease():Promise<Release|null>{const release=await latestRelease();return newerVersion(release.version,currentVersion)?release:null;}

export async function downloadUpdate(release:Release):Promise<DownloadedUpdate> {
  if(!isTauri())throw new Error('Download updates from the installed Windows app.');
  if(!release.downloadable)throw new Error('This release has no verified Windows installer. Open GitHub releases.');
  try{return await invoke<DownloadedUpdate>('download_github_update',{expectedVersion:release.version});}
  catch(error){throw error instanceof Error?error:new Error(String(error));}
}
export async function installUpdate(update:DownloadedUpdate):Promise<void> {
  if(!isTauri())throw new Error('Install updates from the installed Windows app.');
  try{await invoke('install_github_update',{version:update.version});}
  catch(error){throw error instanceof Error?error:new Error(String(error));}
}
