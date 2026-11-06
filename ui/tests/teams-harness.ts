// Runs the real main.ts module in a DOM/Tauri test shell; no daemon or browser.
import { readFileSync } from 'node:fs';
export class Element {
  children: any[] = []; dataset: any = {}; style: any = { setProperty() {} }; attrs: any = {};
  listeners: Record<string, Function[]> = {}; value = ''; hidden = false; open = false;
  className = ''; textContent = ''; offsetWidth = 0; offsetHeight = 0;
  classList = { add() {}, remove() {}, toggle() {}, contains: () => false };
  set innerHTML(_: string) { this.children = []; } get innerHTML() { return ''; }
  append(...els: any[]) { this.children.push(...els); } appendChild(el: any) { this.append(el); return el; }
  prepend(el: any) { this.children.unshift(el); } replaceChildren(...els: any[]) { this.children = els; }
  insertBefore(el: any, before: any) { this.children.splice(this.children.indexOf(before), 0, el); }
  setAttribute(k: string, v: any) { this.attrs[k] = v; } removeAttribute(k: string) { delete this.attrs[k]; }
  addEventListener(k: string, fn: Function) { (this.listeners[k] ??= []).push(fn); }
  before() {} removeEventListener() {} remove() {} focus() {} blur() {} scrollIntoView() {}
  querySelector(selector: string) { return selector === 'summary' ? new Element() : null; } querySelectorAll() { return []; } closest() { return null; }
  getBoundingClientRect() { return {width: 1000, height: 700, x: 0, y: 0, left:0, top:0}; }
  async fire(k: string, extra: any = {}) { for (const fn of this.listeners[k] ?? []) await fn({target:this, currentTarget:this, stopPropagation(){}, preventDefault(){}, ...extra}); }
}
export async function harness(invoke: (cmd: string, args?: any) => Promise<any>, storage = new Map<string,string>(), negative = false, windows = false) {
  const ids = new Map<string,Element>();
  const doc: any = { body: new Element(), documentElement: new Element(), createElement: () => new Element(),
    createTextNode: (s: string) => s, getElementById: (id: string) => { if (!ids.has(id)) ids.set(id, new Element()); return ids.get(id); },
    querySelectorAll: () => [], querySelector: () => null, addEventListener() {} };
  const localStorage = {getItem:(k:string) => storage.get(k) ?? null, setItem:(k:string,v:string) => storage.set(k,v), removeItem:(k:string) => storage.delete(k)};
  const safeInvoke = async (cmd: string, args?: any) => {
    if (cmd === 'feed_list') return {items:[]};
    return invoke(cmd,args);
  };
  const events = new Map<string, Function>();
  const windowEvents = new Map<string, Function>();
  const win: any = {__TAURI__: {core: {invoke:safeInvoke}, event: {listen: async (name:string, handler:Function) => { events.set(name,handler); return () => {}; }}}, addEventListener(name:string,handler:Function){windowEvents.set(name,handler);}, removeEventListener(){}, innerWidth:1000, innerHeight:700, getSelection:()=>null, matchMedia:()=>({matches:false,addEventListener(){}})};
  let source = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  const names: string[] = [], values: any[] = [];
  for (const match of source.matchAll(/^import \{ (.+) \} from "(.+)";$/gm)) {
    const path = match[2];
    const mod: any = path.startsWith('.') ? await import(new URL('../src/'+path.slice(2)+'.ts',import.meta.url).href)
      : path.includes('notification') ? {isPermissionGranted:async()=>true,requestPermission:async()=>{},sendNotification(){}}
      : path.includes('addon-fit') ? {FitAddon:class {fit(){}}}
      : {Terminal:class {options={}; _core={coreService:{onUserInput(){}}}; rows=24; cols=80; buffer={active:{viewportY:0,baseY:0}}; loadAddon(){} open(){} attachCustomKeyEventHandler(){} onUserInput(){} onData(){} write(_:any, cb?:Function){cb?.();} focus(){} dispose(){}}};
    for (const token of match[1].split(',').map(s=>s.trim()).filter(s=>!s.startsWith('type '))) { names.push(token); values.push(mod[token]); }
  }
  source = source.replace(/^import .*;\n/gm,'').replace(/start\(\)\.catch\(\(e\) => \{[\s\S]*?\n\}\);\s*$/, '');
  if (negative) source = source.replace('const managed = managedWorkspaceForSocket(sk);', 'const managed = current();');
  source += `\nreturn {render, renderWsTabs, refreshPaneTitles, saveLayout, collectSids, transferPaneToWs, movePane, attachDividerDrag, wsGroupCtxItems,
    set(ws,ts){workspaces=ws; teams=ts; activeWs=0; wsCounter=100; started=true; hierarchyEnabled=true; configureHierarchyNaming(JSON.stringify({roles:{master:{name:'Alpha',label:'개발본부·책임자'},worker:{name:'B',label:'제작본부·워커'}}}));},
    active(id){activeWs=workspaces.findIndex(w=>w.id===id);}, reorder(){workspaces.reverse();},
    state(){return workspaces;}, paneKeys(){return [...panes.keys()];}, reload:()=>restoreWorkspaces(), loadTeams:()=>loadTeams(), starters:()=>offerTeamStarters(),
    watch:()=>watchTeams(), ready(){teamsReady=true;}, allowSave(){layoutWritable=true;}, create:(...args)=>saveTeam(...args), remove:(...args)=>deleteTeam(...args)};`;
  const js = new Bun.Transpiler({loader:'ts'}).transformSync(source);
  const run = new Function(...names,'window','document','localStorage','navigator','requestAnimationFrame','setInterval','ResizeObserver','getComputedStyle',js);
  const api = run(...values,win,doc,localStorage,{platform:windows?'Win32':'MacIntel',userAgent:windows?'Windows':'Macintosh'},()=>{},()=>0,class {observe(){} disconnect(){}},()=>({getPropertyValue:()=>''}));
  return {api, ids, storage, events, windowEvents};
}
