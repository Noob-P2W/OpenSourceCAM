import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {ready,workspace,ribbon,section,state,acceptNextConfirmation,closeDialogs,importModel,idle} from './test-ui.mjs';

const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
 const page=await browser.newPage({viewport:{width:1440,height:1000}});
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(process.env.PWA_URL);await ready(page);await workspace(page);
 async function openProject(project){
  await closeDialogs(page);await section(page,'project');void acceptNextConfirmation(page);
  await page.locator('#open').setInputFiles({name:'diagnostics.cam.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(project))});
  await page.locator('#status').filter({hasText:'Project opened'}).waitFor();
  await idle(page);await closeDialogs(page);
 }
 const project=await state(page);
 project.part=null;project.workspace=[];project.operations=project.operations.slice(0,1);
 Object.assign(project.operations[0],{strategy:'outside_profile',model_link:null,x:20,y:1,width:20,height:10,depth:2});
 Object.assign(project.operations[0].settings,{path:[],closed:false});
 project.machine.enforce_limits=false;
 await openProject(project);
 await importModel(page,new URL('../examples/models/example-vise.obj',import.meta.url).pathname,'fixture');
 await closeDialogs(page);
 await ribbon(page,'Actions','Generate paths');
 await page.locator('#status').filter({hasText:'Paths generated for inspection'}).waitFor({timeout:60000});
 const issues=page.locator('#issues-panel');
 await issues.waitFor();
 const collision=issues.locator('details').filter({hasText:'Fixture contact'}).first();
 await collision.locator('summary').click();
 assert.match(await collision.locator('.issue-coordinates').textContent(),/Move \d+ · .* Work X /);
 await collision.getByRole('button',{name:'Look at',exact:true}).click();
 const canvas=page.locator('#canvas');
 await page.waitForFunction(()=>{
  const data=document.querySelector('#canvas').dataset;
  return data.issueMove&&/^(cutter|shaft|holder)$/.test(data.issueSection)&&Number(data.toolTriangles)>0&&Number(data.selectionCopies)>0;
 });
 assert.match(await canvas.getAttribute('data-issue-section'),/cutter|shaft|holder/);
 assert.ok(Number(await canvas.getAttribute('data-selection-copies'))>0,'The fixture patch is highlighted');
 await collision.getByRole('button',{name:'Approve',exact:true}).click();
 const confirmation=page.locator('#app-confirm');
 await confirmation.getByRole('button',{name:'Cancel',exact:true}).click();
 await collision.getByRole('button',{name:'Approve',exact:true}).click();
 await confirmation.getByRole('button',{name:'Approve',exact:true}).click();
 await issues.getByText('Show reviewed findings').waitFor();
 await issues.getByRole('checkbox',{name:'Show reviewed findings'}).check();
 await issues.getByText('Reviewed',{exact:false}).last().waitFor();
 await ribbon(page,'Actions','Post / export');
 await page.locator('#nc-preview').click();
 await page.locator('#status.error').filter({hasText:'contacts fixture'}).waitFor();

 const travel=await state(page);
 travel.workspace=[];travel.machine.enforce_limits=true;travel.machine.axis_min=[-500,-500,-200];travel.machine.axis_max[0]=15;
 await openProject(travel);
 await ribbon(page,'Actions','Generate paths');
 await page.locator('#status').filter({hasText:'Paths generated for inspection'}).waitFor();
 await ribbon(page,'Inspect','Issues');
 const limit=issues.locator('details').filter({hasText:'Machine travel limit'}).first();
 await limit.locator('summary').click();
 await limit.getByRole('button',{name:'Open settings',exact:true}).click();
 const field=page.locator('#active-machine-dialog').getByLabel('axis max X · mm',{exact:true});
 await field.waitFor();assert.match(await field.getAttribute('class'),/issue-setting/);
 assert.deepEqual(errors,[]);
 console.log('PASS: issue list, collision preview, move and tool/fixture highlight, review confirmation, retained export preflight and linked machine field');
}finally{await browser.close()}
