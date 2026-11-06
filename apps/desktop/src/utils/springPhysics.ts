export class LyricSpring {
  x = 0; // Current scroll offset (pixels)
  v = 0; // Velocity
  target = 0; // Target line center offset

  private k = 180; // Stiffness
  private c = 24; // Damping coefficient
  private m = 1.0; // Mass

  step(dt: number) {
    // Clamp delta time to avoid explosions during tab suspension
    const clampedDt = Math.min(dt, 0.032);

    // RK4 integration for ultra-smooth physical motion
    const evaluate = (x: number, v: number, dt: number, dx: number, dv: number) => {
      const nextX = x + dx * dt;
      const nextV = v + dv * dt;
      const force = -this.k * (nextX - this.target) - this.c * nextV;
      return { dx: nextV, dv: force / this.m };
    };

    const a = evaluate(this.x, this.v, 0, 0, 0);
    const b = evaluate(this.x, this.v, clampedDt * 0.5, a.dx, a.dv);
    const c = evaluate(this.x, this.v, clampedDt * 0.5, b.dx, b.dv);
    const d = evaluate(this.x, this.v, clampedDt, c.dx, c.dv);

    const dxdt = (1.0 / 6.0) * (a.dx + 2.0 * (b.dx + c.dx) + d.dx);
    const dvdt = (1.0 / 6.0) * (a.dv + 2.0 * (b.dv + c.dv) + d.dv);

    this.x += dxdt * clampedDt;
    this.v += dvdt * clampedDt;
  }
}
