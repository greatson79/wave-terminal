import {spawn} from 'node:child_process';
import {once} from 'node:events';

export async function launchGui(path,args=[]) {
 const child=spawn(path,args,{detached:true,stdio:'ignore',windowsHide:true});
 await once(child,'spawn');
 child.unref();
 return {pid:child.pid,path};
}
