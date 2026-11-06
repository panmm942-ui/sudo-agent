import {expect,test,type Page} from '@playwright/test';
async function host(page:Page){
 await page.route('**/__extension-test',route=>route.fulfill({contentType:'text/html',body:'<!doctype html><meta http-equiv="Content-Security-Policy" content="frame-src \'none\'"><body></body>'}));
 await page.goto('/__extension-test');
 await page.evaluate(async()=>{
  // @ts-expect-error Vite serves this TypeScript source as a browser module.
  const runtime=await import('/src/island/extensionRuntime.ts');
  // @ts-expect-error Vite serves this TypeScript source as a browser module.
  const {EXAMPLE_TAB}=await import('/src/island/extensions.ts');
  const w=window as any;w.extensionRuntime=runtime;w.exampleTab=EXAMPLE_TAB;
  w.mountExtension=(tab=EXAMPLE_TAB)=>{w.extensionCleanup?.();document.querySelector('iframe')?.remove();const frame=document.createElement('iframe');frame.title=tab.name;frame.sandbox.add('allow-scripts');frame.width='318';frame.height='210';document.body.append(frame);const session=crypto.randomUUID();w.extensionCleanup=runtime.attachExtension(frame,tab,session);frame.srcdoc=runtime.extensionDocument(tab,session);};w.mountExtension();
 });
}
test('executes imported JavaScript, buttons and persistent host state after remount',async({page})=>{
 await host(page);const frame=page.frameLocator('iframe');await expect(frame.locator('#status')).toHaveText('Saved on this computer');
 await frame.getByRole('button',{name:'Add one'}).click();await expect(frame.locator('#count')).toHaveText('1');await expect(frame.locator('#status')).toHaveText('Progress saved');
 await page.evaluate(()=>(window as any).mountExtension());await expect(frame.locator('#count')).toHaveText('1');
 await frame.getByRole('button',{name:'Reset'}).click();await expect(frame.locator('#count')).toHaveText('0');
 expect(await frame.locator('body').evaluate(e=>({width:e.clientWidth,scrollWidth:e.scrollWidth}))).toEqual({width:318,scrollWidth:318});
});
test('blocks undeclared networking, host access, external frame navigation and popups',async({page})=>{
 const outbound:string[]=[];page.on('request',request=>{if(request.url().includes('attacker.invalid'))outbound.push(request.url());});
 await host(page);await page.evaluate(()=>{const w=window as any;w.mountExtension({...w.exampleTab,html:'<main><button id="navigate">Navigate</button><p id="checks"></p></main>',css:'',js:`(async()=>{const checks=[];try{parent.document.body;checks.push('HOST_OPEN')}catch{checks.push('host-blocked')}try{localStorage.getItem('sudo.preferences.v1');checks.push('STORAGE_OPEN')}catch{checks.push('storage-blocked')}try{await fetch('https://attacker.invalid/direct');checks.push('FETCH_OPEN')}catch{checks.push('fetch-blocked')}try{await sudo.request({url:'https://attacker.invalid/bridge'});checks.push('BRIDGE_OPEN')}catch{checks.push('bridge-blocked')}document.querySelector('#checks').textContent=checks.join(',');document.querySelector('#navigate').addEventListener('click',()=>{window.open('https://attacker.invalid/popup');location.href='https://attacker.invalid/navigation'})})();`});});
 const frame=page.frameLocator('iframe');await expect(frame.locator('#checks')).toHaveText('host-blocked,storage-blocked,fetch-blocked,bridge-blocked');
 const dialogs:unknown[]=[];page.context().on('page',popup=>dialogs.push(popup));await frame.getByRole('button',{name:'Navigate'}).click();await page.waitForTimeout(300);
 expect(outbound).toEqual([]);expect(dialogs).toEqual([]);expect(page.url()).toContain('/__extension-test');
});
test('blocks same-origin URL navigation while allowing srcdoc code',async({page})=>{
 const attempted:string[]=[];page.on('request',request=>{if(request.url().includes('__forbidden-native-document'))attempted.push(request.url());});
 await host(page);await page.evaluate(()=>{const w=window as any;w.mountExtension({...w.exampleTab,html:'<button id="navigate">Navigate locally</button>',css:'',js:'document.querySelector("#navigate").addEventListener("click",()=>{location.href="http://127.0.0.1:4395/__forbidden-native-document"})'});});
 await page.frameLocator('iframe').getByRole('button',{name:'Navigate locally'}).click();await page.waitForTimeout(200);
 expect(attempted).toEqual([]);expect(page.url()).toContain('/__extension-test');
});
