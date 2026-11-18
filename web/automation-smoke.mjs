import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {ready,editOperation,saveOperation,state} from './test-ui.mjs';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
 const page=await browser.newPage({viewport:{width:1440,height:1000}});await page.goto(process.env.PWA_URL);await ready(page);
 await page.locator('#software-settings').click();await page.getByLabel('Automatically generate toolpaths after changes',{exact:true}).check();await page.locator('#save-software').click();
 await editOperation(page);await page.getByLabel('depth · mm',{exact:true}).fill('2');await saveOperation(page);await page.locator('#status').filter({hasText:'Paths generated and checked'}).waitFor({timeout:60000});
 await editOperation(page);await page.getByRole('tab',{name:'Passes',exact:true}).click();await page.getByLabel('Milling direction',{exact:true}).selectOption('optimized');await saveOperation(page);await page.locator('#status').filter({hasText:'Paths generated and checked'}).waitFor({timeout:60000});assert.equal((await state(page)).operations[0].settings.milling_mode,'optimized');
 console.log('PASS: automatic generation after committed edits and optimized milling mode persistence');
}finally{await browser.close()}
