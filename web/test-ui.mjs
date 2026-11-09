export async function section(page,pane){
 if(await page.locator('#home').isVisible())await page.locator('#resume-workspace').click();
 const target=pane==='project'?'import':pane==='model'?'import':pane;
 const button=page.locator(`[data-pane-target="${target}"]`);
 if(await button.isVisible())await button.click();
 if(pane==='project'&&!await page.locator('.file-menu').evaluate(el=>el.open))await page.locator('.file-menu > summary').click();
}
