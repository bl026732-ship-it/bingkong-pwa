/* 生成 PWA 图标：橙底饼干 + 巧克力豆（纯几何绘制，无需字体） */
const fs = require('fs');
const path = require('path');
const { PNG } = require('pngjs');

const COOKIE = [255, 179, 64];    // #FFB340
const CHIP = [139, 90, 43];       // #8B5A2B
const BG = [15, 15, 19];          // #0F0F13

function drawIcon(size, maskable) {
  const png = new PNG({ width: size, height: size });
  const c = size / 2;
  // maskable：饼干占 72% 安全区，其余铺底色；普通版：饼干铺满整图
  const radius = maskable ? size * 0.36 : size * 0.5;
  // 巧克力豆：黄金角均匀分布 7 颗（保证互不粘连）
  const chips = [];
  for (let i = 0; i < 7; i++) {
    const a = i * 2.39996 + 0.7;                 // 黄金角
    const r = radius * (0.2 + 0.52 * ((i * 0.37 + 0.11) % 1));
    chips.push([c + Math.cos(a) * r, c + Math.sin(a) * r]);
  }
  const chipR = radius * 0.12;

  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const idx = (size * y + x) << 2;
      const d = Math.hypot(x + 0.5 - c, y + 0.5 - c);
      let rgb = BG;
      if (d <= radius) {
        rgb = COOKIE;
        for (const [cx, cy] of chips) {
          if (Math.hypot(x + 0.5 - cx, y + 0.5 - cy) <= chipR) { rgb = CHIP; break; }
        }
        // 边缘轻微深色描边，增加立体感
        if (d > radius - size * 0.012) rgb = [235, 152, 40];
      }
      png.data[idx] = rgb[0];
      png.data[idx + 1] = rgb[1];
      png.data[idx + 2] = rgb[2];
      png.data[idx + 3] = 255;
    }
  }
  return PNG.sync.write(png);
}

const out = path.join(__dirname, 'icons');
fs.writeFileSync(path.join(out, 'icon-192.png'), drawIcon(192, false));
fs.writeFileSync(path.join(out, 'icon-512.png'), drawIcon(512, false));
fs.writeFileSync(path.join(out, 'icon-512-maskable.png'), drawIcon(512, true));
console.log('icons generated:', fs.readdirSync(out).join(', '));
