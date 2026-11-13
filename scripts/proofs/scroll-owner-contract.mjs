import {classifyViewport} from './scroll-contract.mjs';
export function classifyOwnerStream(observation, olderHistoryAvailable=true) {
 const unknown=sees=>({verdict:'NOT EXECUTED',sees});
 if(observation?.error || observation?.helperError || observation?.captureError || !observation?.input)return unknown('observer/input unavailable');
 const before={source:observation.source,markers:observation.preInputMarkers};
 const frames=observation.frames;
 if(!Array.isArray(frames) || frames.length!==4)return unknown('timed frames incomplete');
 const targets=[50,150,700,2800];
 const baselineAge=Date.parse(observation.inputEndedAt)-Date.parse(observation.preInputTime);
 if(!Number.isFinite(baselineAge) || baselineAge<0 || baselineAge>100 || frames.some((f,i)=>f.elapsedMs<targets[i] || f.elapsedMs>targets[i]+50))return unknown('capture schedule or adjacent baseline unproven');
 const states=frames.map(frame=>classifyViewport(before,frame));
 if(states.includes('NOT_PROVEN') || frames.some((f,i)=>!Number.isFinite(f.elapsedMs) || f.elapsedMs<0 || (i>0 && f.elapsedMs<=frames[i-1].elapsedMs)))return unknown('invalid frame/timestamp evidence');
 if(!olderHistoryAvailable)return {verdict:'NOT RED',sees:'nothing to scroll'};
 const selectedIndex=states.indexOf('MOVED_UP');
 if(selectedIndex<0)return {verdict:'RED',sees:observation.input.kind==='native-key'?'keyboard scroll did not respond: no older history in captured samples':'wheel dead: no older-history response in captured samples'};
 const selected=frames[selectedIndex],last=frames.at(-1);
 if(last.elapsedMs-selected.elapsedMs<2000)return unknown('older history appeared too late to measure 2s retention');
 const later=frames.slice(selectedIndex+1);
 if(later.some(frame=>frame.markers[0]>selected.markers[0]))return {verdict:'RED',sees:'jumps back toward newest output',selectedMarker:selected.markers[0]};
 if(later.some(frame=>frame.markers[0]<selected.markers[0]))return unknown('unexpected older-frame drift; retention oracle unproven');
 return {verdict:'NOT RED',sees:'older history remains readable for >=2s',selectedMarker:selected.markers[0],retentionMs:last.elapsedMs-selected.elapsedMs};
}

export function hasActiveOwnerProducer(counters, observation, calibrationRows) {
 const start=Date.parse(observation.preInputTime),end=Date.parse(observation.frames?.at(-1)?.time),early=Date.parse(observation.frames?.[0]?.time);
 if(!Number.isFinite(start) || !Number.isFinite(end) || !Number.isFinite(early) || !Number.isFinite(calibrationRows) || calibrationRows<10 || counters.length<3)return false;
 if(counters.some((row,i)=>!Number.isFinite(row.at) || !Number.isInteger(row.burst) || !Number.isInteger(row.line) || row.line>=5000 || (i>0 && (row.at<=counters[i-1].at || row.at-counters[i-1].at>300 || row.burst!==counters[i-1].burst+1 || row.line!==counters[i-1].line+5))))return false;
 if(counters[0].at>start || counters.at(-1).at<end)return false;
 const during=counters.filter(row=>row.at>start && row.at<=end);
 if(during.length<2 || during[0].at-start>300 || end-during.at(-1).at>300)return false;
 // Output before the first sample must not hide the calibrated idle movement.
 return counters.filter(row=>row.at>start && row.at<=early).length*5<calibrationRows;
}
