export function checkSteer(
  idle: boolean,
  current: number,
  expected: number,
): void {
  if (idle) throw new Error("invalid_state: Steer requires an active run");
  if (current !== expected)
    throw new Error("stale_turn: top-level run changed");
}
