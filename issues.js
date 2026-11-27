import {confirmAction} from './dialogs.js';

export function createIssuesPanel({root, look, settings, operationName, onClose, onCount}) {
 let items=new Map(),reviewed=new Set(),expanded=new Set(),showReviewed=false,scanned=false;
 const panel=root.closest('#issues-panel'),summary=panel.querySelector('#issues-summary');
 function button(label,action,cls=''){
  const control=document.createElement('button');control.type='button';control.textContent=label;
  if(cls)control.className=cls;control.onclick=action;return control;
 }
 function render(){
  const active=[...items.values()].filter(issue=>!reviewed.has(issue.id));
  const count=active.length;onCount(count);
  summary.textContent=count?`${count} open ${count===1?'issue':'issues'}`:reviewed.size?'All findings reviewed':scanned?'No issues found':'Checking workspace…';
  const controls=panel.querySelector('#issues-reviewed');controls.hidden=!reviewed.size;
  controls.querySelector('input').checked=showReviewed;
  const rows=showReviewed?[...items.values()]:active;
  root.replaceChildren();
  if(!rows.length)return;
  const table=document.createElement('table');
  table.className='issues-table';
  table.setAttribute('aria-label','Workspace issues');
  const body=document.createElement('tbody');table.append(body);root.append(table);
  for(const issue of rows){
   const row=body.insertRow(),cell=row.insertCell(),details=document.createElement('details');
   const label=document.createElement('summary'),title=document.createElement('span'),scope=document.createElement('span');
   const done=reviewed.has(issue.id),index=issue.location?.operation??issue.target.index;
   label.dataset.severity=done?'reviewed':issue.severity;
   title.className='issue-title';title.textContent=issue.title;
   scope.className='issue-scope';
   const owner=Number.isInteger(index)?operationName(index):null;
   scope.textContent=[done?'Reviewed':issue.severity==='warning'?'Warning':'Error',owner].filter(Boolean).join(' · ');
   label.append(title,scope);details.append(label);
   details.open=expanded.has(issue.id);
   details.ontoggle=()=>details.open?expanded.add(issue.id):expanded.delete(issue.id);
   const explanation=document.createElement('p');explanation.textContent=issue.detail;
   const remedy=document.createElement('p');remedy.className='issue-remedy';remedy.textContent=issue.remedy;
   const actions=document.createElement('div');actions.className='issue-actions';
   if(issue.location){
    actions.append(button('Look at',()=>look(issue.location)));
    actions.append(button('Open settings',()=>settings(issue.target)));
   }else{
    actions.append(button('Look at',()=>settings(issue.target)));
   }
   actions.append(button(done?'Reopen':'Approve',async()=>{
    if(done){reviewed.delete(issue.id);render();return}
    if(await confirmAction({title:'Approve this finding?',message:'Mark this finding as reviewed for the current workspace. It will be hidden from the open list. NC export still runs its machine and collision checks.',accept:'Approve',destructive:true})){
     if(items.get(issue.id)!==issue)return;
     reviewed.add(issue.id);render();
    }
   },done?'':'issue-approve'));
   details.append(explanation);
   if(issue.location){
    const coordinates=document.createElement('p'),p=issue.location.point;
    coordinates.className='issue-coordinates';
    coordinates.textContent=`Move ${issue.location.move_index+1} · ${issue.location.seconds.toFixed(3)} s · Work X ${p[0].toFixed(3)} Y ${p[1].toFixed(3)} Z ${p[2].toFixed(3)} mm`;
    details.append(coordinates);
   }
   details.append(remedy,actions);cell.append(details);
  }
 }
 panel.querySelector('#close-issues').onclick=onClose;
 panel.querySelector('#issues-reviewed input').onchange=event=>{showReviewed=event.target.checked;render()};
 return {
  scanned:()=>scanned,
  count:()=>[...items.keys()].filter(id=>!reviewed.has(id)).length,
  show(on){panel.hidden=!on;document.body.dataset.issues=String(on);if(on)render()},
  replace(list){
   items=new Map(list.map(issue=>[issue.id,issue]));scanned=true;
   reviewed=new Set([...reviewed].filter(id=>items.has(id)));
   render();
  },
  scan(list){for(const issue of list){items.set(issue.id,issue)}scanned=true;render()},
  add(issue){items.set(issue.id,issue);render()},
  reset(){items.clear();reviewed.clear();expanded.clear();scanned=false;showReviewed=false;render()},
 };
}
