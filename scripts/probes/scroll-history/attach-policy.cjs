// Wire model of src-tauri/src/main.rs::stream_end_action; this does not run the GUI.
const REATTACH_MAX=5,REATTACH_DELAY_MS=500;
function streamEndAction(startPid,nowPid,surface,tries) {
  if(startPid===null||startPid===undefined)return 'exited';
  if(nowPid===null||nowPid===undefined)return tries<REATTACH_MAX?'wait':'daemon-restarted';
  if(nowPid!==startPid)return 'daemon-restarted';
  return surface?.exited===false&&tries<REATTACH_MAX?'reattach':'exited';
}
async function reattachAfterEnd(startPid,readState,attach,sleep,record) {
  let tries=0;
  while(true) {
    const {pid,surface}=await readState();
    const action=streamEndAction(startPid,pid,surface,tries);
    record({action,tries,startPid,pid});
    if(action!=='reattach'&&action!=='wait')throw new Error(`automatic reattach stopped: ${action}`);
    tries++;
    await sleep(REATTACH_DELAY_MS);
    if(action==='reattach')try{return await attach();}catch(error){record({attachError:String(error),tries});}
  }
}
module.exports={streamEndAction,reattachAfterEnd};
