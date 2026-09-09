/**
 * Deterministic SVG identicon from an agent public key. No dependency, no image
 * hosting — the key's own bytes seed a mirrored 5×5 blockie.
 */
export function identiconSvg(seed: Uint8Array, size = 32): string {
  const at = (i: number) => seed[i % Math.max(seed.length, 1)] ?? 0;

  const hue = (at(1) * 359) % 360;
  const hue2 = (hue + 40 + (at(2) % 60)) % 360;
  const fg = `hsl(${hue} 64% 62%)`;
  const accent = `hsl(${hue2} 70% 58%)`;

  const n = 5;
  const half = Math.ceil(n / 2);
  let rects = "";
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < half; x++) {
      const bit = at(4 + y * half + x);
      if ((bit & 1) === 0) continue;
      const fill = (bit & 2) === 0 ? fg : accent;
      for (const xx of x === n - 1 - x ? [x] : [x, n - 1 - x]) {
        rects += `<rect x="${xx}" y="${y}" width="1.02" height="1.02" fill="${fill}"/>`;
      }
    }
  }

  return (
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${n} ${n}" ` +
    `width="${size}" height="${size}" shape-rendering="crispEdges" role="img" aria-hidden="true">` +
    `<rect width="${n}" height="${n}" rx="1" fill="hsl(${hue} 30% 16%)"/>${rects}</svg>`
  );
}
