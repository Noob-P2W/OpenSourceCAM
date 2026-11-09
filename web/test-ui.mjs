export async function section(page,pane){const button=page.locator(`[data-pane-target="${pane}"]`);if(await button.isVisible())await button.click()}
