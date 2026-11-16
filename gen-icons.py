# -*- coding: utf-8 -*-
"""
饼控自律 · 图标生成器
=====================
设计：深可可圆角底板 + 金色烤饼 + 外圈计时弧 + 6 颗巧克力豆
输出：PWA 图标（icons/）与 Tauri 桌面图标（src-tauri/icons/，含多尺寸 .ico/.icns）

特点：每个尺寸都从同一套几何按比例重新绘制（4x 超采样后降采样），
      不依赖放大，所以 16px / 32px 依然干净。

用法：python gen-icons.py
"""
import os
import math
import numpy as np
from PIL import Image

ROOT = os.path.dirname(os.path.abspath(__file__))
ICONS = os.path.join(ROOT, 'icons')
TAURI = os.path.join(ROOT, 'src-tauri', 'icons')

# ---------- 配色 ----------
BG_TOP = (58, 36, 24)       # #3A2418
BG_BOT = (21, 13, 9)        # #150D09
GLOW = (255, 138, 59)
COOKIE_LIGHT = (243, 190, 105)
COOKIE_MID = (226, 152, 52)
COOKIE_DARK = (198, 118, 28)
RIM = (188, 108, 26)
CHIP = (62, 35, 18)
CHIP_HI = (110, 67, 32)
ARC = (255, 126, 45)
TRACK = (255, 255, 255)


def _lerp(a, b, t):
    a = np.array(a, dtype=np.float32)
    b = np.array(b, dtype=np.float32)
    return a + (b - a) * t[..., None]


def _circle_soft(dist, r, soft):
    return np.clip((r - dist) / soft + 0.5, 0, 1)


def render(size, maskable=False):
    """返回 size×size 的 PIL RGBA 图。"""
    ss = 8 if size <= 64 else 4
    n = size * ss
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    x = (xx + 0.5) / n
    y = (yy + 0.5) / n
    px = 1.0 / n

    s = 0.78 if maskable else 1.0          # maskable 内容收进安全区
    cx = cy = 0.5

    # ---------- 底板 ----------
    rgb = _lerp(BG_TOP, BG_BOT, np.clip((y - 0.06) / 0.9, 0, 1))
    if maskable:
        tile = np.ones_like(x)
    else:
        r = 0.225
        qx = np.abs(x - 0.5) - (0.5 - r)
        qy = np.abs(y - 0.5) - (0.5 - r)
        d = np.hypot(np.maximum(qx, 0), np.maximum(qy, 0)) + np.minimum(np.maximum(qx, qy), 0) - r
        tile = (d <= 0).astype(np.float32)

    # 中心暖光
    gd = np.hypot(x - 0.5, y - 0.46) / 0.62
    glow = np.exp(-(gd ** 2) * 2.1)[..., None] * np.array(GLOW, dtype=np.float32) / 255.0
    rgb = np.clip(rgb / 255.0 + glow * 0.30, 0, 1)

    # 顶部 1px 高光边，避免深色板在深色任务栏里“糊”掉
    # maskable 是满幅出血、会被系统裁形，这条边反而成了接缝，故跳过
    if not maskable:
        top_edge = np.exp(-((y - 0.012) ** 2) / (2 * (0.008 ** 2))) * tile
        rgb = np.clip(rgb + top_edge[..., None] * 0.10, 0, 1)

    # ---------- 计时弧 ----------
    ring_r = 0.402 * s
    ring_w = 0.028 * s
    dr = np.hypot(x - cx, y - cy)
    ring_mask = np.abs(dr - ring_r) <= ring_w / 2

    ang = (np.degrees(np.arctan2(y - cy, x - cx)) + 90.0) % 360.0
    sweep = 245.0
    arc_mask = ring_mask & (ang <= sweep)

    # 圆头端帽
    for a_deg in (0.0, sweep):
        rad = math.radians(a_deg - 90.0)
        ex, ey = cx + math.cos(rad) * ring_r, cy + math.sin(rad) * ring_r
        arc_mask |= np.hypot(x - ex, y - ey) <= ring_w / 2

    rgb = np.where((ring_mask & ~arc_mask)[..., None], rgb * 0.72 + 0.28 * (np.array(TRACK) / 255.0) * 0.30, rgb)
    rgb = np.where(arc_mask[..., None], np.array(ARC, dtype=np.float32) / 255.0, rgb)

    # ---------- 饼干 ----------
    R = 0.332 * s
    ang_c = np.arctan2(y - cy, x - cx)
    cd = np.hypot(x - cx, y - cy)
    # 轻微不规则边缘：读起来是“烤出来的饼”，不是硬币
    wob = 1.0 + 0.010 * np.sin(3.1 * ang_c + 1.2) + 0.006 * np.sin(5.7 * ang_c + 0.4)
    inside = cd <= R * wob

    # 落在底板上的柔和阴影
    shadow = (np.exp(-(((cd - R) / (0.030 * s)) ** 2)) * 0.55 * (cd > R))[..., None]
    rgb = np.clip(rgb * (1 - shadow), 0, 1)

    # 双段渐变：亮 → 中 → 暗（左上受光）
    t = np.clip(0.46 + ((x - cx) * 0.62 + (y - cy) * 0.86) * 1.55, 0, 1)
    tt = t[..., None]
    cookie = np.where(tt < 0.55,
                      np.array(COOKIE_LIGHT, np.float32) + (np.array(COOKIE_MID, np.float32) - np.array(COOKIE_LIGHT, np.float32)) * (tt / 0.55),
                      np.array(COOKIE_MID, np.float32) + (np.array(COOKIE_DARK, np.float32) - np.array(COOKIE_MID, np.float32)) * ((tt - 0.55) / 0.45))
    cookie = cookie / 255.0

    # 烤边：外缘压暗（哑光），不要亮圈
    cd_n = cd / (R * wob)
    edge = np.clip((cd_n - 0.76) / 0.24, 0, 1)
    cookie = cookie * (1 - (edge ** 1.35)[..., None] * 0.42)

    # 大面积柔光（模拟斜上方光源打在表面）
    soft = np.clip(1 - np.hypot(x - (cx - 0.09 * s), y - (cy - 0.10 * s)) / (0.46 * s), 0, 1) ** 2
    cookie = np.clip(cookie + (soft * 0.055)[..., None] * np.array((255, 238, 200), np.float32) / 255.0, 0, 1)

    # 极轻的烘焙颗粒感（固定伪随机，避免每次不同）
    n2 = np.sin(x * 1417.7 + y * 2311.3) * np.sin(x * 733.1 - y * 1867.9)
    cookie = np.clip(cookie + (n2 * 0.016)[..., None], 0, 1)

    rgb = np.where(inside[..., None], cookie, rgb)

    # 饼干边缘细描（最外一圈），小尺寸下保住轮廓
    rim = np.clip((cd_n - 0.955) / 0.045, 0, 1) * inside
    rgb = np.where((rim > 0)[..., None], rgb * (1 - rim[..., None] * 0.22), rgb)

    # ---------- 巧克力碎块（不规则，带投影与高光，不是贴片） ----------
    chips = [
        (-0.098, -0.108, 0.052, 18),
        (0.101, -0.117, 0.047, -22),
        (0.141, 0.053, 0.045, 8),
        (0.013, 0.143, 0.048, 35),
        (-0.141, 0.049, 0.046, -12),
        (-0.054, 0.034, 0.031, 26),
    ]
    nn = 2.9
    for dx, dy, cr, rot in chips:
        ccx, ccy = cx + dx * s, cy + dy * s
        rr = cr * s
        ca, sa = math.cos(math.radians(rot)), math.sin(math.radians(rot))
        u, v = x - ccx, y - ccy
        uu = u * ca + v * sa
        vv = -u * sa + v * ca
        a_, b_ = rr * 1.08, rr * 0.93
        q = ((np.abs(uu) / a_) ** nn + (np.abs(vv) / b_) ** nn) ** (1.0 / nn)

        # 下方投影，让碎块“嵌”进饼里
        u2, v2 = x - ccx, y - (ccy + rr * 0.22)
        uu2 = u2 * ca + v2 * sa
        vv2 = -u2 * sa + v2 * ca
        q2 = ((np.abs(uu2) / a_) ** nn + (np.abs(vv2) / b_) ** nn) ** (1.0 / nn)
        # 从芯片边缘连续衰减，不留“未受影的亮缝”
        sh = (0.34 * np.exp(-(np.clip(q2 - 1.0, 0, None) / 0.55) ** 2))[..., None]
        rgb = np.clip(rgb * (1 - sh), 0, 1)

        # 本体
        hl = np.clip(1 - np.hypot(uu + a_ * 0.32, vv + b_ * 0.36) / (rr * 1.25), 0, 1) ** 1.7
        col = np.array(CHIP, np.float32) + (np.array(CHIP_HI, np.float32) - np.array(CHIP, np.float32)) * (hl[..., None] * 0.58)
        col = col / 255.0 * (0.85 + 0.15 * (1 - q)[..., None])
        rgb = np.where((q <= 1)[..., None], col, rgb)

    # ---------- 全局极轻的顶光 ----------
    rgb = np.clip(rgb * (1.0 + 0.05 * np.clip((0.4 - y) / 0.4, 0, 1))[..., None], 0, 1)

    out = np.dstack([rgb, tile]).astype(np.float32)
    img = Image.fromarray((np.clip(out, 0, 1) * 255 + 0.5).astype(np.uint8), 'RGBA')
    return img.resize((size, size), Image.BOX)   # 面积平均：无振铃，不会在硬边产生亮圈


def save_png(img, path):
    img.save(path, 'PNG', optimize=True)


def build_ico(path):
    """多尺寸 ICO：小尺寸用 BMP(DIB) 保证兼容，大尺寸用 PNG 压缩。"""
    sizes = [16, 24, 32, 48, 64, 128, 256]
    frames = {s: render(s) for s in sizes}
    entries = []

    def dib(im):
        import io
        w, h = im.size
        buf = io.BytesIO()
        # BITMAPINFOHEADER
        import struct
        header = struct.pack('<IiiHHIIiiII', 40, w, h * 2, 1, 32, 0, w * h * 4, 0, 0, 0, 0)
        px = im.load()
        body = bytearray()
        for yy in range(h - 1, -1, -1):          # bottom-up
            for xx in range(w):
                r, g, b, a = px[xx, yy]
                body += bytes((b, g, r, a))
        # AND mask：透明处为 1
        row_bytes = ((w + 31) // 32) * 4
        mask = bytearray()
        for yy in range(h - 1, -1, -1):
            bits = bytearray(row_bytes)
            for xx in range(w):
                if px[xx, yy][3] == 0:
                    bits[xx // 8] |= (0x80 >> (xx % 8))
            mask += bits
        return header + bytes(body) + bytes(mask)

    def png_bytes(im):
        import io
        b = io.BytesIO()
        im.save(b, 'PNG', optimize=True)
        return b.getvalue()

    blobs = []
    for s in sizes:
        blobs.append(png_bytes(frames[s]) if s >= 128 else dib(frames[s]))

    import struct
    count = len(sizes)
    out = bytearray(struct.pack('<HHH', 0, 1, count))
    offset = 6 + 16 * count
    for s, blob in zip(sizes, blobs):
        out += struct.pack('<BBBBHHII', 0 if s >= 256 else s, 0 if s >= 256 else s, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
    for blob in blobs:
        out += blob
    open(path, 'wb').write(bytes(out))


def build_icns(path):
    try:
        big = render(1024)
        big.save(path, 'ICNS')  # Pillow 会写入多尺寸
        return True
    except Exception as e:
        print('  icns 生成失败（可忽略，Windows 不用）：', e)
        return False


def main():
    os.makedirs(ICONS, exist_ok=True)
    os.makedirs(TAURI, exist_ok=True)

    # PWA
    save_png(render(192), os.path.join(ICONS, 'icon-192.png'))
    save_png(render(512), os.path.join(ICONS, 'icon-512.png'))
    save_png(render(512, maskable=True), os.path.join(ICONS, 'icon-512-maskable.png'))
    print('PWA 图标完成')

    # Tauri
    mapping = {
        '32x32.png': 32, '64x64.png': 64, '128x128.png': 128,
        '128x128@2x.png': 256, 'icon.png': 512,
        'Square30x30Logo.png': 30, 'Square44x44Logo.png': 44, 'Square71x71Logo.png': 71,
        'Square89x89Logo.png': 89, 'Square107x107Logo.png': 107, 'Square142x142Logo.png': 142,
        'Square150x150Logo.png': 150, 'Square284x284Logo.png': 284, 'Square310x310Logo.png': 310,
        'StoreLogo.png': 50,
    }
    for name, s in mapping.items():
        save_png(render(s), os.path.join(TAURI, name))
    build_ico(os.path.join(TAURI, 'icon.ico'))
    build_icns(os.path.join(TAURI, 'icon.icns'))
    print('Tauri 图标完成（含 icon.ico 16/24/32/48/64/128/256）')

    # 预览图，方便肉眼检查
    prev = Image.new('RGBA', (860, 300), (15, 15, 19, 255))
    x = 20
    for s in (256, 128, 64, 32, 16):
        im = render(s)
        prev.paste(im, (x, 20 + (256 - s) // 2), im)
        x += s + 24
    prev.save(os.path.join(ROOT, '_icon-preview.png'))
    print('预览图：_icon-preview.png')


if __name__ == '__main__':
    main()
