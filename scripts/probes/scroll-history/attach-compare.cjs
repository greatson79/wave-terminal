// Compare only a shared live window; snapshots and ConPTY input bytes are not a reference.
function compareWindow(reference, client, begin, end) {
  function window(bytes) {
    const start = bytes.indexOf(begin);
    const stop = bytes.indexOf(end);
    if (!begin.length || !end.length || start < 0 || stop < start + begin.length ||
        bytes.indexOf(begin, start + 1) >= 0 || bytes.indexOf(end, stop + 1) >= 0) return null;
    return bytes.subarray(start + begin.length, stop);
  }
  const expected = window(reference), received = window(client);
  if (expected === null || received === null) return { comparable:false, reason:'missing, reversed or duplicate marker' };
  let cursor = 0;
  for (const byte of expected) if (cursor < received.length && byte === received[cursor]) cursor++;
  return {
    comparable:true,
    equal:expected.equals(received),
    referenceBytes:expected.length,
    clientBytes:received.length,
    missingBytes:expected.length - received.length,
    omission:received.length < expected.length && cursor === received.length,
  };
}
module.exports = { compareWindow };
