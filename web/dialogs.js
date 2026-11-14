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

// One floating action menu for project entries. No inline action lists.
let contextMenu,contextOwner;
function closeContextMenu(restore=false){contextMenu?.remove();contextMenu=null;if(restore&&contextOwner?.isConnected)contextOwner.focus();contextOwner=null}
export function showContextMenu(items,x,y,owner){
 document.dispatchEvent(new Event('cam-context-menu-open'));closeContextMenu();
 const available=items.filter(Boolean);if(!available.length)return;
 const menu=document.createElement('div');menu.id='project-context-menu';menu.className='project-context-menu';menu.setAttribute('role','menu');menu.setAttribute('aria-label','Project entry actions');contextMenu=menu;contextOwner=owner;
 for(const item of available){if(typeof item.checked==='boolean'){const label=document.createElement('label'),input=document.createElement('input');input.type='checkbox';input.checked=item.checked;input.disabled=item.disabled===true;input.onchange=()=>{closeContextMenu();item.action()};label.append(input,document.createTextNode(item.label));menu.append(label);continue}const b=document.createElement('button');b.type='button';b.setAttribute('role','menuitem');b.textContent=item.label;b.disabled=item.disabled===true;b.onclick=()=>{closeContextMenu();item.action()};menu.append(b)}
 document.body.append(menu);const r=menu.getBoundingClientRect();menu.style.left=`${Math.max(8,Math.min(x,innerWidth-r.width-8))}px`;menu.style.top=`${Math.max(8,Math.min(y,innerHeight-r.height-8))}px`;
 const enabled=()=>[...menu.querySelectorAll('button:not(:disabled),input:not(:disabled)')];enabled()[0]?.focus();
 menu.onkeydown=e=>{const buttons=enabled(),index=buttons.indexOf(document.activeElement);if(['ArrowDown','ArrowUp','Home','End'].includes(e.key)){e.preventDefault();buttons[e.key==='Home'?0:e.key==='End'?buttons.length-1:(index+(e.key==='ArrowDown'?1:buttons.length-1))%buttons.length]?.focus()}else if(e.key==='Escape'){e.preventDefault();closeContextMenu(true)}else if(e.key==='Tab')closeContextMenu()};
}
export function bindContextMenu(target,getItems){
 target._contextItems=getItems;if(target.dataset.contextBound)return;target.dataset.contextBound='true';
 target.setAttribute('aria-haspopup','menu');target.title='Right-click for actions';
 const open=(x,y)=>showContextMenu(target._contextItems(),x,y,target);
 target.oncontextmenu=e=>{e.preventDefault();e.stopPropagation();open(e.clientX,e.clientY)};
 target.addEventListener('keydown',e=>{if(e.key==='ContextMenu'||(e.shiftKey&&e.key==='F10')){e.preventDefault();const r=target.getBoundingClientRect();open(r.left,r.bottom)}});
 let timer,start,suppressClick=false;target.addEventListener('click',e=>{if(suppressClick){suppressClick=false;e.preventDefault();e.stopImmediatePropagation()}},true);
 target.addEventListener('pointerdown',e=>{if(e.pointerType!=='touch')return;start=[e.clientX,e.clientY];timer=setTimeout(()=>{timer=null;suppressClick=true;open(...start)},550)});
 const clear=()=>{clearTimeout(timer);timer=null;if(suppressClick)setTimeout(()=>{suppressClick=false},0)};target.addEventListener('pointermove',e=>{if(start&&Math.hypot(e.clientX-start[0],e.clientY-start[1])>10)clear()});for(const type of ['pointerup','pointercancel','lostpointercapture'])target.addEventListener(type,clear);
}
document.addEventListener('pointerdown',e=>{if(contextMenu&&!contextMenu.contains(e.target))closeContextMenu()});
document.addEventListener('cam-context-menu-open',()=>closeContextMenu());
document.addEventListener('scroll',e=>{if(!contextMenu?.contains(e.target))closeContextMenu()},true);window.addEventListener('resize',()=>closeContextMenu());
