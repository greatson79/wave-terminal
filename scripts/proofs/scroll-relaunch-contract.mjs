import {classifyViewport} from './scroll-contract.mjs';

export function classifyRelaunch({guiGone, control, beforeIdentity, afterIdentity, sameSeat, baseline, attempts}) {
 if (guiGone !== true || !control || !sameSeat || !Number.isInteger(beforeIdentity?.daemon_pid) ||
     beforeIdentity.daemon_pid !== afterIdentity?.daemon_pid ||
     !Number.isFinite(beforeIdentity.started_at) || beforeIdentity.started_at !== afterIdentity?.started_at)
  return 'NOT EXECUTED';
 const valid = attempt => ['wheel','ShiftPageUp'].includes(attempt?.action) && attempt.input && !attempt.error && !attempt.helperError &&
  classifyViewport(baseline, attempt) !== 'NOT_PROVEN';
 // A single valid route proves retained history; RED needs both tested routes.
 if (attempts.some(a => valid(a) && classifyViewport(baseline, a) === 'MOVED_UP')) return 'PASS';
 return ['wheel','ShiftPageUp'].every(action =>
  attempts.some(a => a.action === action && valid(a))) ? 'RED' : 'NOT EXECUTED';
}
