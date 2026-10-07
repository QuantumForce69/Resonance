/** Leading + trailing throttle: fires now, then at most once per `ms` with the latest args. */
export function throttle<A extends unknown[]>(fn: (...args: A) => void, ms: number): (...args: A) => void {
  let last = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let pending: A | null = null;
  const fire = (args: A) => {
    last = Date.now();
    fn(...args);
  };
  return (...args: A) => {
    const wait = ms - (Date.now() - last);
    if (wait <= 0) {
      if (timer) {
        clearTimeout(timer);
        timer = null;
      }
      pending = null;
      fire(args);
      return;
    }
    pending = args;
    if (!timer) {
      timer = setTimeout(() => {
        timer = null;
        if (pending) {
          const p = pending;
          pending = null;
          fire(p);
        }
      }, wait);
    }
  };
}
