type InputTerminal = {
  _core?: { coreService?: { onUserInput?: (listener: () => void) => unknown } };
};

export function inputSource(term: InputTerminal, warn: (message: string) => void = console.warn) {
  const subscribe = term._core?.coreService?.onUserInput;
  if (typeof subscribe !== "function") {
    warn("xterm onUserInput unavailable; treating all terminal input as human");
    return { takeHuman: () => true };
  }
  let human = false;
  subscribe(() => { human = true; });
  return { takeHuman: () => {
    const current = human;
    human = false;
    return current;
  } };
}
