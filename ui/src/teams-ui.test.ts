import { expect, test } from 'bun:test';
import { harness } from '../tests/teams-harness';
const team = (id:string,name=id) => ({id,name,folder:'/fixture/'+id,runtime_key:'team-'+id,stopped:false,created_at:'2026-11-06T12:00:00Z'});
const socket = (id:string) => '/fixture/cys-dept-team-'+id+'/cys.sock';
async function sidebarRegression(negative = false) {
  const teams = [team('A'),team('B'),team('C','같은 팀'),team('D','같은 팀')];
  let title = 'B·개발본부·worker·s123';
  let unblock!:()=>void, entered!:()=>void; let delayed = true;
  const attaching = new Promise<void>(r=>entered=r);
  const gate = new Promise<void>(r=>unblock=r);
  const {api,ids,storage} = await harness(async (cmd,args) => {
    if(cmd==='team_list') return structuredClone(teams);
    if(cmd==='list_depts') return {depts:Object.fromEntries(teams.map(t=>[t.runtime_key,{socket:socket(t.id)}]))};
    if(cmd==='attach_surface' && args.socket===socket('A') && delayed) {entered();await gate;return {output_event:'out-A',exited_event:'exit-A'};}
    if(cmd==='team_update') {const i=teams.findIndex(t=>t.id===args.id);teams[i]={...teams[i],...args.patch};return teams[i];}
    if(cmd==='team_create') {const added=team('E',args.name);teams.push(added);return added;}
    if(cmd==='team_delete') {teams.splice(teams.findIndex(t=>t.id===args.id),1);return;}
    if(cmd==='list_surfaces') { return {surfaces:args.socket===socket('A')?[{surface_id:123,title,role:'worker',exited:false,live_cwd:'/fixture/A'}]:[]}; }
    if(cmd==='org_status') return {surfaces:[]};
    return {};
  }, new Map(), negative);
  api.set(teams.map((t,i)=>({id:i+1,teamId:t.id,name:t.name,socket:socket(t.id),tree:null})),structuredClone(teams));
  api.active(2);
  const pending=api.refreshPaneTitles();
  await attaching; api.active(3); api.reorder(); api.active(2); delayed=false; unblock(); await pending;
  const check = () => {
    expect(api.state().find((w:any)=>w.teamId==='A').tree).toEqual({type:'pane',sid:123});
    expect(api.state().filter((w:any)=>w.teamId!=='A').flatMap((w:any)=>api.collectSids(w.tree))).not.toContain(123);
    expect(api.state()).toHaveLength(4);
    expect(ids.get('ws-tabs')!.children.some((t:any)=>t.dataset.teamId==='A')).toBe(true);
  };
  check(); title='전혀 다른 한글 · Alpha · 개발본부 · s123'; await api.refreshPaneTitles(); check();
  await api.create({name:'이름 변경',folder:'/fixture/A'},'A');
  api.state().find((w:any)=>w.teamId==='A').collapsed=true;
  await api.remove('B');await api.create({name:'A',folder:'/fixture/E'});check();
  expect(api.state().find((w:any)=>w.teamId==='A').socket).toBe(socket('A'));
  expect(teams.find(t=>t.id==='A')!.folder).toBe('/fixture/A');
  api.allowSave(); api.saveLayout(); expect(storage.has('cys-layout-v3')).toBe(true);
  await api.reload(); check();
}
test('real main wiring binds delayed adoption to team id across reorder, rename, save and reload', () => sidebarRegression());

test('negative control restores active-tab routing in an isolated main.ts copy and the SAME regression fails', async () => {
  await expect(sidebarRegression(true)).rejects.toThrow('expect(received).toEqual(expected)');
});
test('fresh starters are suggestions only; skip-all persists and upgrades never auto-offer', async () => {
  let offered=false;
  const calls:string[]=[];
  const invoke=async(cmd:string)=>{calls.push(cmd); if(cmd==='team_store_status') return {schema_version:1,starters_offered:offered,corrupt:null}; if(cmd==='team_mark_starters_offered'){offered=true;return;} if(cmd==='team_list')return []; return {depts:{},surfaces:[]};};
  const {api,ids}=await harness(invoke);
  await api.loadTeams(); await api.starters();
  const panel=ids.get('team-starters')!;
  expect(panel.children.map((e:any)=>e.textContent)).toEqual(['기획팀','제작팀','검토팀','건너뛰기']);
  expect(calls).not.toContain('team_create');
  await panel.children[3].fire('click'); expect(offered).toBe(true); expect(panel.hidden).toBe(true);
  const again=await harness(invoke); await again.api.loadTeams(); await again.api.starters();
  expect(again.ids.get('team-starters')?.children ?? []).toHaveLength(0);
  offered=false;
  const upgrade=await harness(invoke,new Map([['cys-layout-v2','{"workspaces":[]}']]));
  await upgrade.api.loadTeams(); await upgrade.api.starters();
  expect(upgrade.ids.get('team-starters')?.children ?? []).toHaveLength(0);
});
test('boot/render preserves v2 bytes and does not start or stop any team runtime', async () => {
  const original=JSON.stringify({workspaces:[{id:9,name:'old workspace',tree:{type:'pane',sid:9},manual:true}],groups:[]});
  const calls:string[]=[];
  const {api,storage,ids}=await harness(async(cmd,args)=>{calls.push(cmd);if(cmd==='team_list')return [team('A')];if(cmd==='team_store_status')return {starters_offered:false,corrupt:null};if(cmd==='list_surfaces')return {surfaces:!args.socket?[{surface_id:9,title:'old',exited:false}]:[]};return {depts:{},surfaces:[]};},new Map([['cys-layout-v2',original]]));
  await api.reload(); api.render();
  expect(storage.get('cys-layout-v2')).toBe(original); expect(storage.has('cys-layout-v3')).toBe(false);
  expect(calls.filter(c=>/launch|allocate|stop_dept|new_surface/.test(c))).toEqual([]);
  expect(api.state().find((w:any)=>w.teamId==='A').tree).toBe(null);
  expect(ids.get('ws-tabs')!.children.some((el:any)=>el.textContent==='기존 작업 공간')).toBe(true);
});
test('CLI events and window focus re-read the store; rename uses the exact patch contract', async () => {
  let list=[team('A')]; const calls:{cmd:string,args:any}[]=[];
  const {api,events,windowEvents,ids}=await harness(async(cmd,args)=>{
    calls.push({cmd,args});
    if(cmd==='team_list')return list;
    if(cmd==='team_store_status')return {starters_offered:true,corrupt:null};
    if(cmd==='team_update') {expect(args).toEqual({id:'A',patch:{name:'수정',folder:'/fixture/A'}});return {...team('A'),...args.patch};}
    return {depts:{},surfaces:[]};
  });
  await api.watch(); await api.loadTeams(); api.ready();
  list=[team('A'),team('B')]; await events.get('teams-changed')!({payload:{revision:99}});
  expect(api.state().map((w:any)=>w.teamId)).toContain('B');
  list=[team('A'),team('C')]; await windowEvents.get('focus')!();
  expect(api.state().map((w:any)=>w.teamId)).toEqual(['A','C']);
  await api.create({name:'수정',folder:'/fixture/A'},'A');
  expect(api.state().find((w:any)=>w.teamId==='A').name).toBe('수정');
  expect(calls.filter(c=>c.cmd==='team_list')).toHaveLength(3);
  expect(ids.get('ws-tabs')!.children.some((el:any)=>el.dataset.teamId==='C')).toBe(true);
});
test('same surface number on different sockets stays independent; managed panes cannot transfer', async () => {
  const teams=[team('A'),team('B')];
  const {api}=await harness(async(cmd,args)=>cmd==='list_surfaces'?{surfaces:args.socket?[{surface_id:7,title:'B · Alpha · s123',role:'worker',exited:false}]:[]}: {depts:{},surfaces:[]});
  api.set(teams.map((t,i)=>({id:i+1,teamId:t.id,name:t.name,socket:socket(t.id),tree:null})),structuredClone(teams));
  await api.refreshPaneTitles(); api.active(1);api.transferPaneToWs(7,2);
  expect(api.state().map((w:any)=>api.collectSids(w.tree))).toEqual([[7],[7]]);
});
test('deleted team late list response cannot create a title-routed legacy workspace', async () => {
  const teams=[team('A'),team('B')];
  let entered!:()=>void,release!:()=>void;
  const pending=new Promise<void>(r=>entered=r), gate=new Promise<void>(r=>release=r);
  const {api}=await harness(async(cmd,args)=>{
    if(cmd==='list_surfaces' && args.socket===socket('A')) {entered();await gate;return {surfaces:[{surface_id:123,title:'B·제작본부·worker·s123',role:'worker',exited:false}]};}
    return {depts:{},surfaces:[]};
  });
  api.set(teams.map((t,i)=>({id:i+1,teamId:t.id,name:t.name,socket:socket(t.id),tree:null})),structuredClone(teams));
  const refresh=api.refreshPaneTitles();await pending;await api.remove('A');release();await refresh;
  expect(api.state().map((w:any)=>w.teamId)).toEqual(['B']);
  expect(api.paneKeys()).toHaveLength(0);
});
test('CLI deletion disposes UI listeners and runtime entries without closing runtime seats', async () => {
  let list=[team('A')];const calls:string[]=[];
  const {api,events}=await harness(async(cmd,args)=>{
    calls.push(cmd);
    if(cmd==='team_list')return list;
    if(cmd==='team_store_status')return {starters_offered:true,corrupt:null};
    if(cmd==='list_surfaces')return {surfaces:args.socket?[{surface_id:8,title:'Alpha·開発·s8',role:'worker',exited:false}]:[]};
    return {depts:{},surfaces:[]};
  });
  api.set([{id:1,teamId:'A',name:'A',socket:socket('A'),tree:null}],list);
  await api.refreshPaneTitles();expect(api.paneKeys()).toHaveLength(1);
  await api.watch();api.ready();list=[];await events.get('teams-changed')!({payload:{}});
  expect(api.state()).toHaveLength(0);expect(api.paneKeys()).toHaveLength(0);
  expect(calls.filter(c=>/stop_dept|close_surface/.test(c))).toEqual([]);
});
test('socket rebound during pending attach cannot commit an old runtime seat into the new socket', async () => {
  let bound=socket('A');let entered!:()=>void, release!:()=>void;
  const waiting=new Promise<void>(r=>entered=r), gate=new Promise<void>(r=>release=r);
  const {api}=await harness(async(cmd,args)=>{
    if(cmd==='team_list')return [team('A')];
    if(cmd==='team_store_status')return {starters_offered:true,corrupt:null};
    if(cmd==='list_depts')return {depts:{'team-A':{socket:bound}}};
    if(cmd==='list_surfaces')return {surfaces:args.socket===socket('A')?[{surface_id:123,title:'Alpha·開発·s123',role:'worker',exited:false}]:[]};
    if(cmd==='attach_surface' && args.socket===socket('A')) {entered();await gate;return {};}
    return {surfaces:[]};
  });
  api.set([{id:1,teamId:'A',name:'A',socket:socket('A'),tree:null}],[team('A')]);
  const refresh=api.refreshPaneTitles();await waiting;bound='/fixture/new-runtime/cys.sock';await api.loadTeams();release();await refresh;
  expect(api.state()[0].socket).toBe(bound);expect(api.state()[0].tree).toBe(null);expect(api.paneKeys()).toHaveLength(0);
});
test('first explicit divider, pane and group edits after restart persist without a prior tab click', async () => {
  const setup=async()=>{
    const h=await harness(async()=>({surfaces:[],depts:{}}));
    h.api.set([{id:1,name:'legacy',manual:true,tree:{type:'split',dir:'row',a:{type:'pane',sid:1},b:{type:'pane',sid:2}}}],[]);
    return h;
  };
  const divider=await setup();
  const {Element}=await import('../tests/teams-harness');const node=divider.api.state()[0].tree;
  const handle=new Element();divider.api.attachDividerDrag(handle,new Element(),node,new Element(),new Element());
  await handle.fire('mousedown');await divider.windowEvents.get('mousemove')!({clientX:700,clientY:0});await divider.windowEvents.get('mouseup')!();
  expect(divider.storage.has('cys-layout-v3')).toBe(true);
  expect(JSON.parse(divider.storage.get('cys-layout-v3')!).workspaces[0].tree.ratio).toBe(0.7);
  const pane=await setup();pane.api.movePane(2,1,'left');
  expect(JSON.parse(pane.storage.get('cys-layout-v3')!).workspaces[0].tree.a.sid).toBe(2);
  const group=await setup();group.api.wsGroupCtxItems(group.api.state()[0])[0].action();
  expect(JSON.parse(group.storage.get('cys-layout-v3')!).groups).toHaveLength(1);
});
