// Application confirmations stay in the workspace and follow its theme.
export function confirmAction({title='Confirm action',message,accept='Continue',destructive=false}){
 return new Promise(resolve=>{
  const previous=document.activeElement,dialog=document.createElement('dialog');
  dialog.id='app-confirm';dialog.className='work-dialog confirm-dialog';dialog.setAttribute('aria-labelledby','confirm-title');dialog.setAttribute('aria-describedby','confirm-message');
  const head=document.createElement('div');head.className='dialog-head';const heading=document.createElement('h2');heading.id='confirm-title';heading.textContent=title;head.append(heading);
  const body=document.createElement('div');body.className='dialog-body';const text=document.createElement('p');text.id='confirm-message';text.textContent=message;body.append(text);
  const footer=document.createElement('div');footer.className='dialog-footer';const cancel=document.createElement('button'),ok=document.createElement('button');cancel.textContent='Cancel';cancel.dataset.confirm='false';ok.textContent=accept;ok.dataset.confirm='true';ok.className=destructive?'danger-action':'primary';footer.append(cancel,ok);dialog.append(head,body,footer);
  let settled=false;
  const finish=value=>{if(settled)return;settled=true;dialog.close();dialog.remove();if(previous?.isConnected&&!previous.disabled)previous.focus();resolve(value)};
  cancel.onclick=()=>finish(false);ok.onclick=()=>finish(true);dialog.addEventListener('cancel',event=>{event.preventDefault();finish(false)});dialog.addEventListener('close',()=>finish(false));
  document.body.append(dialog);dialog.showModal();cancel.focus();
 });
}
