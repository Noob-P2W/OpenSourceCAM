export async function section(page,pane){
 const target=pane==='project'?'import':pane==='model'?'import':pane;
 const button=page.locator(`[data-pane-target="${target}"]`);
 if(await button.isVisible())await button.click();
 if(pane==='project'&&!await page.locator('.file-menu').evaluate(el=>el.open))await page.locator('.file-menu > summary').click();
}
