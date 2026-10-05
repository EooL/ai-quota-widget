import { useState } from "react";
import { useInterval } from "./useInterval";

export function useNow(intervalMs = 1000) {
  const [now, setNow] = useState(() => Date.now());
  useInterval(() => setNow(Date.now()), intervalMs);
  return now;
}
