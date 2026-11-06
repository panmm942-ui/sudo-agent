/** Physics time shared by input events and display callbacks, both from performance.now(). */
export class MotionClock {
  private last: number | null = null;

  /** A backdrop wake must not move an already-running animation's time origin. */
  wake(now: number): void { this.last ??= now; }

  step(now: number): number {
    if (this.last === null) return 0;
    const dt = Math.max(0, Math.min((now - this.last) / 1000, .1));
    this.last = now;
    return dt;
  }

  /** Consume this elapsed time under the OLD target before assigning its replacement. */
  retarget(now: number): number {
    const dt = this.step(now);
    this.wake(now);
    return dt;
  }

  sleep(): void { this.last = null; }
}
