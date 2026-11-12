import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {ready,importModel,state,closeDialogs} from './test-ui.mjs';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
const fixture=new URL('../tests/fixtures/vise-assembly.step',import.meta.url).pathname;
const part=new URL('../examples/models/mount.stl',import.meta.url).pathname;
try{
 for(const viseFirst of [true,false]){
  const context=await browser.newContext({viewport:{width:768,height:1024}}),page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));await page.goto(process.env.PWA_URL);await ready(page);
  if(!viseFirst)await importModel(page,part);
  const initial=await state(page);await importModel(page,fixture,'vise');const withVise=await state(page);
  assert.deepEqual(withVise.part,initial.part,'Adding a vise preserves the workpiece');assert.deepEqual(withVise.operations,initial.operations,'Adding a vise preserves operations');assert.equal(withVise.workspace.length,initial.workspace.length+1);
  const reference=withVise.workspace.at(-1);await importModel(page,viseFirst?part:new URL('../examples/models/mount.step',import.meta.url).pathname);const combined=await state(page);
  assert.ok(combined.part);assert.deepEqual(combined.workspace,withVise.workspace,'Importing a workpiece preserves vise geometry and placement');assert.deepEqual(combined.machine,withVise.machine);assert.deepEqual(combined.tool_library,withVise.tool_library);assert.equal(combined.operations.length,0,'Old workpiece operations are cleared');
  await closeDialogs(page);await page.locator('#undo').click();await page.waitForFunction(before=>localStorage.getItem('oc-project')===before,JSON.stringify(withVise));await page.locator('#redo').click();await page.waitForFunction(expected=>localStorage.getItem('oc-project')===expected,JSON.stringify(combined));
  await page.reload();await ready(page);const restored=await state(page);assert.deepEqual(restored.workspace.at(-1),reference);assert.deepEqual(restored.part,combined.part);assert.deepEqual(errors,[]);await context.close();
  console.log(`PASS: ${viseFirst?'vise-first':'part-first'} import preserves references, workpiece, CNC/tool library, undo/redo and reload`);
 }
}finally{await browser.close()}
