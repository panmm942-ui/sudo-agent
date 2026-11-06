import {expect,it,vi} from 'vitest';
import {invoke} from '@tauri-apps/api/core';
import {newerVersion,parseRelease,latestRelease,checkRelease,currentVersion,downloadUpdate,installUpdate} from './updates';
vi.mock('@tauri-apps/api/core',()=>({isTauri:()=>true,invoke:vi.fn()}));
it('compares stable release versions numerically and validates the repository URL',()=>{
  expect(newerVersion('v0.10.0','0.9.9')).toBe(true);
  expect(newerVersion('v0.1.0','0.1.0')).toBe(false);
  expect(newerVersion('v1.0.0-beta','0.1.0')).toBe(false);
  expect(parseRelease({tag_name:'v0.2.0',html_url:'https://evil.example/setup.exe'})).toBe(null);
  expect(parseRelease({tag_name:'v0.2.0',html_url:'https://github.com/panmm942-ui/sudo-agent/releases/tag/v0.2.0'})).toMatchObject({version:'v0.2.0'});
});
it('uses the native private-repository check and distinguishes current from newer releases',async()=>{
  vi.mocked(invoke).mockResolvedValue({tag_name:`v${currentVersion}`,html_url:`https://github.com/panmm942-ui/sudo-agent/releases/tag/v${currentVersion}`});
  expect(await latestRelease()).toMatchObject({version:`v${currentVersion}`});
  expect(await checkRelease()).toBe(null);
  expect(invoke).toHaveBeenCalledWith('check_github_release');
  vi.mocked(invoke).mockResolvedValue({tag_name:'v99.0.0',html_url:'https://github.com/panmm942-ui/sudo-agent/releases/tag/v99.0.0'});
  expect(await checkRelease()).toMatchObject({version:'v99.0.0'});
});
it('shows native authentication failures as errors instead of claiming no update',async()=>{
  vi.mocked(invoke).mockRejectedValue('Your GitHub sign-in cannot access this private repository.');
  await expect(checkRelease()).rejects.toThrow('GitHub sign-in');
});
it('offers native installer download only for the exact versioned x64 asset with a GitHub SHA-256 digest',()=>{
  const raw={tag_name:'v99.0.0',html_url:'https://github.com/panmm942-ui/sudo-agent/releases/tag/v99.0.0',assets:[{name:'sudo-island_99.0.0_x64-setup.exe',state:'uploaded',size:1200,id:123,digest:'sha256:'+ 'a'.repeat(64)}]};
  expect(parseRelease(raw)).toMatchObject({downloadable:true});
  expect(parseRelease({...raw,assets:[{...raw.assets[0],digest:null}]})).toMatchObject({downloadable:false});
  expect(parseRelease({...raw,assets:[{...raw.assets[0],name:'sudo-island_98.0.0_x64-setup.exe'}]})).toMatchObject({downloadable:false});
});

it('requires verified release capability and sends only a version to native update commands',async()=>{
  const release={version:'v99.0.0',url:'https://github.com/panmm942-ui/sudo-agent/releases/tag/v99.0.0',downloadable:false};
  vi.mocked(invoke).mockClear();
  await expect(downloadUpdate(release)).rejects.toThrow('no verified Windows installer');
  expect(invoke).not.toHaveBeenCalled();
  vi.mocked(invoke).mockResolvedValue({version:'v99.0.0',path:'C:/Sudo/updates/setup.exe'});
  const downloaded=await downloadUpdate({...release,downloadable:true});
  expect(invoke).toHaveBeenLastCalledWith('download_github_update',{expectedVersion:'v99.0.0'});
  await installUpdate({...downloaded,path:'C:/arbitrary.exe'});
  expect(invoke).toHaveBeenLastCalledWith('install_github_update',{version:'v99.0.0'});
});
