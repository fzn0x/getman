// The app's pixel icons, drawn on a 7×7 grid and shown at whole multiples
// so every pixel lands on the screen's. `#` is ink.

const art = {
  search: [".###...", "#...#..", "#...#..", "#...#..", ".###...", "....##.", ".....##"],
  contrast: ["..###..", ".#.###.", "#..####", "#..####", "#..####", ".#.###.", "..###.."],
  copy: ["..#####", "..#...#", "#####.#", "#...#.#", "#...###", "#...#..", "#####.."],
  check: [".......", "......#", ".....#.", "#...#..", ".#.#...", "..#....", "......."],
  close: ["#.....#", ".#...#.", "..#.#..", "...#...", "..#.#..", ".#...#.", "#.....#"],
  play: ["#......", "###....", "#####..", "#######", "#####..", "###....", "#......"],
  menu: [".......", "#######", ".......", "#######", ".......", "#######", "......."],
  right: ["...#...", "....#..", ".....#.", "#######", ".....#.", "....#..", "...#..."],
  left: ["...#...", "..#....", ".#.....", "#######", ".#.....", "..#....", "...#..."],
  file: ["####...", "#..##..", "#...##.", "#....#.", "#....#.", "#....#.", "######."],
  folder: ["###....", "#..####", "#.....#", "#.....#", "#.....#", "#.....#", "#######"],
};

/** An inline SVG icon; `size` is 7, 14 or 21. */
export function icon(name, size = 14) {
  const rows = art[name];
  if (!rows) throw new Error(`no icon named ${name}`);
  const rects = [];
  rows.forEach((row, y) => {
    for (let x = 0; x < row.length;) {
      if (row[x] !== "#") {
        x += 1;
        continue;
      }
      let w = 1;
      while (row[x + w] === "#") w += 1;
      rects.push(`<rect x="${x}" y="${y}" width="${w}" height="1"/>`);
      x += w;
    }
  });
  return `<svg class="icon" viewBox="0 0 7 7" width="${size}" height="${size}" shape-rendering="crispEdges" fill="currentColor" aria-hidden="true">${rects.join("")}</svg>`;
}
