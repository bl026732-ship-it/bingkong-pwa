/* 贴边收起的时间球
   —— 复用 pip 窗口，不新增窗口
   —— 形态：Free 264×124 小窗 / Capsule 贴边胶囊(≈1/3) / Ball 96×96 完整球 / Menu 菜单面板
   —— 几何、吸附、动画、穿透开关全部在 Rust 侧；pip.html 只负责画和上报点击 */

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

/* ---------- 临时调试（BALL_DEBUG=1 时写 %TEMP%\ball_debug.log） ---------- */
fn dbg(msg: String) {
    if std::env::var("BALL_DEBUG").is_err() {
        return;
    }
    use std::io::Write;
    let p = std::env::temp_dir().join("ball_debug.log");
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{}", msg);
    }
}

/* ---------- 尺寸（逻辑像素，乘 scale 得物理像素） ---------- */
const FREE_W: f64 = 264.0;
const FREE_H: f64 = 124.0;
const BALL: f64 = 96.0;
const CAP_W_V: f64 = 34.0; // 左右贴边时的可见宽度：96 的约 1/3
const CAP_H_V: f64 = 96.0;
const CAP_W_H: f64 = 96.0; // 上下贴边
const CAP_H_H: f64 = 34.0;
const MENU_W: f64 = 178.0;
const MENU_H: f64 = 236.0;

const SNAP: f64 = 60.0; // 中心点距边 <60 就吸附
const HOVER_IN: u64 = 300; // 悬停多久展开
const HOVER_OUT: u64 = 500; // 移开多久收回
const DRAG_QUIET: u64 = 260; // 多久没移动算拖动结束

/* ---------- 类型 ---------- */
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Free,
    Capsule,
    Ball,
    Menu,
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(Clone, Serialize, Deserialize)]
struct Persist {
    docked: bool,
    edge: Edge,
    free_x: i32,
    free_y: i32,
    #[serde(default = "neg1")]
    cx: i32, // 贴边时的窗口中心（沿边方向），重开时能回到原位
    #[serde(default = "neg1")]
    cy: i32,
    #[serde(default)]
    visible: bool, // 上次退出时悬浮窗开着？下次启动自动恢复
    opacity: u8,
}

fn neg1() -> i32 {
    -1
    }

impl Default for Persist {
    fn default() -> Self {
        Self { docked: false, edge: Edge::Right, free_x: -1, free_y: -1, cx: -1, cy: -1, visible: false, opacity: 85 }
    }
}

struct S {
    mode: Mode,
    edge: Edge,
    opacity: u8,
    prev_mode: Mode,
    prev_rect: Option<Rect>,
    hover_since: Option<Instant>,
    leave_since: Option<Instant>,
    last_move: Option<Instant>,
    quiet_until: Option<Instant>,
    menu: bool,
    dragging: bool,
}

impl S {
    fn new() -> Self {
        Self {
            mode: Mode::Free,
            edge: Edge::Right,
            opacity: 85,
            prev_mode: Mode::Free,
            prev_rect: None,
            hover_since: None,
            leave_since: None,
            last_move: None,
            quiet_until: None,
            menu: false,
            dragging: false,
        }
    }
}

static ST: Mutex<Option<S>> = Mutex::new(None);
static ANIM: AtomicU32 = AtomicU32::new(0);
static TICK_N: AtomicU32 = AtomicU32::new(0);

/* 拿状态：顺带把 Option 拆成 &mut S，调用处直接 g.mode 就行 */
struct St<'a>(std::sync::MutexGuard<'a, Option<S>>);

impl std::ops::Deref for St<'_> {
    type Target = S;
    fn deref(&self) -> &S {
        self.0.as_ref().expect("ball state")
    }
}
impl std::ops::DerefMut for St<'_> {
    fn deref_mut(&mut self) -> &mut S {
        self.0.as_mut().expect("ball state")
    }
}

fn st() -> St<'static> {
    let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = Some(S::new());
    }
    St(g)
}

#[derive(Clone, Serialize)]
pub struct Ev {
    mode: Mode,
    edge: Edge,
    opacity: u8,
}

/* ---------- 配置：写用户配置目录，绝不写程序目录 ---------- */
fn cfg_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("ball.json"))
}

fn load_cfg(app: &AppHandle) -> Persist {
    let Some(p) = cfg_path(app) else { return Persist::default() };
    let Ok(txt) = fs::read_to_string(&p) else { return Persist::default() };
    serde_json::from_str(&txt).unwrap_or_default()
}

fn save_cfg(app: &AppHandle, p: &Persist) {
    let Some(path) = cfg_path(app) else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(txt) = serde_json::to_string_pretty(p) {
        let _ = fs::write(path, txt);
    }
}

fn persist_now(app: &AppHandle, win: &WebviewWindow) {
    let (mode, edge, opacity) = {
        let g = st();
        (g.mode, g.edge, g.opacity)
    };
    let mut p = load_cfg(app);
    p.docked = mode != Mode::Free;
    p.edge = edge;
    p.opacity = opacity;
    let (cx, cy) = center_of(win);
    p.cx = cx.round() as i32;
    p.cy = cy.round() as i32;
    if !p.docked {
        if let Ok(pos) = win.outer_position() {
            p.free_x = pos.x;
            p.free_y = pos.y;
        }
    }
    save_cfg(app, &p);
}

/* ---------- 几何 ---------- */
fn wa_at(app: &AppHandle, x: f64, y: f64) -> Rect {
    let m = app
        .monitor_from_point(x, y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(m) = m {
        let wa = m.work_area();
        if wa.size.width > 0 && wa.size.height > 0 {
            return Rect { x: wa.position.x, y: wa.position.y, w: wa.size.width as i32, h: wa.size.height as i32 };
        }
        let p = m.position();
        let s = m.size();
        return Rect { x: p.x, y: p.y, w: s.width as i32, h: s.height as i32 };
    }
    Rect { x: 0, y: 0, w: 1280, h: 800 }
}

fn scale_at(app: &AppHandle, x: f64, y: f64) -> f64 {
    app.monitor_from_point(x, y)
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0)
}

fn clamp_i(v: i32, lo: i32, hi: i32) -> i32 {
    if hi < lo { lo } else { v.max(lo).min(hi) }
}

/* 贴边胶囊：along = 沿边方向的中心坐标（物理，全局坐标） */
fn capsule_rect(wa: Rect, edge: Edge, s: f64, along: f64) -> Rect {
    let (w, h) = match edge {
        Edge::Left | Edge::Right => ((CAP_W_V * s).round() as i32, (CAP_H_V * s).round() as i32),
        Edge::Top | Edge::Bottom => ((CAP_W_H * s).round() as i32, (CAP_H_H * s).round() as i32),
    };
    match edge {
        Edge::Left => Rect { x: wa.x, w, h, y: clamp_i(along.round() as i32 - h / 2, wa.y, wa.y + wa.h - h) },
        Edge::Right => Rect { x: wa.x + wa.w - w, w, h, y: clamp_i(along.round() as i32 - h / 2, wa.y, wa.y + wa.h - h) },
        Edge::Top => Rect { x: clamp_i(along.round() as i32 - w / 2, wa.x, wa.x + wa.w - w), y: wa.y, w, h },
        Edge::Bottom => Rect { x: clamp_i(along.round() as i32 - w / 2, wa.x, wa.x + wa.w - w), y: wa.y + wa.h - h, w, h },
    }
}

fn ball_rect(wa: Rect, edge: Edge, s: f64, along: f64) -> Rect {
    let d = (BALL * s).round() as i32;
    match edge {
        Edge::Left => Rect { x: wa.x, y: clamp_i(along.round() as i32 - d / 2, wa.y, wa.y + wa.h - d), w: d, h: d },
        Edge::Right => Rect { x: wa.x + wa.w - d, y: clamp_i(along.round() as i32 - d / 2, wa.y, wa.y + wa.h - d), w: d, h: d },
        Edge::Top => Rect { x: clamp_i(along.round() as i32 - d / 2, wa.x, wa.x + wa.w - d), y: wa.y, w: d, h: d },
        Edge::Bottom => Rect { x: clamp_i(along.round() as i32 - d / 2, wa.x, wa.x + wa.w - d), y: wa.y + wa.h - d, w: d, h: d },
    }
}

fn rect_of(win: &WebviewWindow) -> Rect {
    let p = win.outer_position().unwrap_or(PhysicalPosition::new(0, 0));
    let s = win.outer_size().unwrap_or(PhysicalSize::new(FREE_W as u32, FREE_H as u32));
    Rect { x: p.x, y: p.y, w: s.width as i32, h: s.height as i32 }
}

fn center_of(win: &WebviewWindow) -> (f64, f64) {
    let r = rect_of(win);
    (r.x as f64 + r.w as f64 / 2.0, r.y as f64 + r.h as f64 / 2.0)
}

/* ---------- 补间动画 ---------- */
fn animate(win: &WebviewWindow, to: Rect, ms: u64) {
    let my_gen = ANIM.fetch_add(1, Ordering::SeqCst) + 1;
    let from = rect_of(win);
    let win = win.clone();
    {
        let mut g = st();
        g.quiet_until = Some(Instant::now() + Duration::from_millis(ms + 220));
    }
    thread::spawn(move || {
        let steps = (ms / 12).max(5);
        for i in 1..=steps {
            if ANIM.load(Ordering::SeqCst) != my_gen {
                return;
            }
            let t = i as f64 / steps as f64;
            let e = 1.0 - (1.0 - t).powi(3); // ease-out
            let x = from.x as f64 + (to.x - from.x) as f64 * e;
            let y = from.y as f64 + (to.y - from.y) as f64 * e;
            let w = from.w as f64 + (to.w - from.w) as f64 * e;
            let h = from.h as f64 + (to.h - from.h) as f64 * e;
            let _ = win.set_size(PhysicalSize::new(w.round().max(1.0) as u32, h.round().max(1.0) as u32));
            let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
            thread::sleep(Duration::from_millis(12));
        }
        let fin = rect_of(&win);
        let inner = win.inner_size().map(|s| (s.width, s.height)).unwrap_or((0, 0));
        dbg(format!("anim target=({},{},{}x{}) outer=({},{},{}x{}) inner={}x{}",
            to.x, to.y, to.w, to.h, fin.x, fin.y, fin.w, fin.h, inner.0, inner.1));
    });
}

fn emit_state(win: &WebviewWindow) {
    let g = st();
    let ev = Ev { mode: g.mode, edge: g.edge, opacity: g.opacity };
    drop(g);
    let _ = win.emit("ball-mode", ev);
}

/* ---------- 形态切换 ---------- */
fn dock_to(app: &AppHandle, win: &WebviewWindow, edge: Edge, along: Option<f64>) {
    let (cx, cy) = center_of(win);
    let wa = wa_at(app, cx, cy);
    let s = scale_at(app, cx, cy);
    let a = along.unwrap_or(match edge {
        Edge::Left | Edge::Right => cy,
        _ => cx,
    });
    let r = capsule_rect(wa, edge, s, a);
    dbg(format!("dock_to {:?} wa=({},{},{}x{}) s={:.3} -> r=({},{},{}x{})", edge, wa.x, wa.y, wa.w, wa.h, s, r.x, r.y, r.w, r.h));
    {
        let mut g = st();
        g.mode = Mode::Capsule;
        g.edge = edge;
        g.hover_since = None;
        g.leave_since = None;
    }
    let _ = win.set_ignore_cursor_events(false);
    animate(win, r, 180);
    emit_state(win);
    persist_now(app, win);
}

fn expand_ball(app: &AppHandle, win: &WebviewWindow) {
    let (cx, cy) = center_of(win);
    let wa = wa_at(app, cx, cy);
    let s = scale_at(app, cx, cy);
    let edge = { st().edge };
    let along = match edge {
        Edge::Left | Edge::Right => cy,
        _ => cx,
    };
    let r = ball_rect(wa, edge, s, along);
    if let Ok(c) = app.cursor_position() {
        dbg(format!("expand_ball {:?} wa=({},{},{}x{}) s={:.3} cur=({:.0},{:.0}) -> r=({},{},{}x{})",
            edge, wa.x, wa.y, wa.w, wa.h, s, c.x, c.y, r.x, r.y, r.w, r.h));
    }
    {
        let mut g = st();
        g.mode = Mode::Ball;
        g.hover_since = None;
        g.leave_since = None;
    }
    animate(win, r, 160);
    emit_state(win);
}

fn collapse(app: &AppHandle, win: &WebviewWindow) {
    let (cx, cy) = center_of(win);
    let wa = wa_at(app, cx, cy);
    let s = scale_at(app, cx, cy);
    let edge = { st().edge };
    let along = match edge {
        Edge::Left | Edge::Right => cy,
        _ => cx,
    };
    let r = capsule_rect(wa, edge, s, along);
    {
        let mut g = st();
        g.mode = Mode::Capsule;
        g.hover_since = None;
        g.leave_since = None;
    }
    let _ = win.set_ignore_cursor_events(false);
    animate(win, r, 160);
    emit_state(win);
}

/* 还原成完整小窗（从球/胶囊拖出来，或菜单里点「还原小窗」） */
fn restore_free(app: &AppHandle, win: &WebviewWindow, cx: f64, cy: f64) {
    let wa = wa_at(app, cx, cy);
    let s = scale_at(app, cx, cy);
    let w = (FREE_W * s).round() as i32;
    let h = (FREE_H * s).round() as i32;
    let r = Rect {
        x: clamp_i(cx.round() as i32 - w / 2, wa.x, wa.x + wa.w - w),
        y: clamp_i(cy.round() as i32 - h / 2, wa.y, wa.y + wa.h - h),
        w,
        h,
    };
    {
        let mut g = st();
        g.mode = Mode::Free;
        g.hover_since = None;
        g.leave_since = None;
    }
    let _ = win.set_ignore_cursor_events(false);
    animate(win, r, 180);
    emit_state(win);
    persist_now(app, win);
}

/* ---------- 拖动结束判定 ---------- */
fn after_drag(app: &AppHandle, win: &WebviewWindow) {
    let (cx, cy) = center_of(win);
    let wa = wa_at(app, cx, cy);
    let s = scale_at(app, cx, cy);
    let snap = SNAP * s;

    let dl = cx - wa.x as f64;
    let dr = (wa.x + wa.w) as f64 - cx;
    let dt = cy - wa.y as f64;
    let db = (wa.y + wa.h) as f64 - cy;
    let mut best = Edge::Left;
    let mut dist = dl;
    if dr < dist {
        dist = dr;
        best = Edge::Right;
    }
    if dt < dist {
        dist = dt;
        best = Edge::Top;
    }
    if db < dist {
        dist = db;
        best = Edge::Bottom;
    }

    if dist < snap {
        dock_to(app, win, best, Some(match best {
            Edge::Left | Edge::Right => cy,
            _ => cx,
        }));
    } else {
        restore_free(app, win, cx, cy);
    }
}

/* 窗口移动（可能来自拖动，也可能来自我们自己的动画） */
pub fn on_moved(app: &AppHandle) {
    let now = Instant::now();
    let mut g = st();
    if let Some(q) = g.quiet_until {
        if now < q {
            return;
        }
    }
    g.last_move = Some(now);
    let _ = app;
}

/* ---------- 60ms 轮询：穿透 / 悬停 ---------- */
fn tick(app: &AppHandle, win: &WebviewWindow) {
    if !win.is_visible().unwrap_or(false) {
        return;
    }
    let now = Instant::now();
    let (mode, menu) = {
        let g = st();
        (g.mode, g.menu)
    };
    if menu {
        return;
    }

    /* 拖动结束：最后一次移动后静默 DRAG_QUIET 毫秒 */
    let last = { st().last_move };
    if let Some(t) = last {
        if now.duration_since(t) >= Duration::from_millis(DRAG_QUIET) {
            {
                let mut g = st();
                g.last_move = None;
                g.dragging = false;
            }
            after_drag(app, win);
            return;
        }
        return; // 正在拖动，不处理悬停
    }

    if let Some(q) = st().quiet_until {
        if now < q {
            return; // 动画中
        }
    }

    let Some(cur) = app.cursor_position().ok() else { return };
    let r = rect_of(win);
    let rx = cur.x - r.x as f64;
    let ry = cur.y - r.y as f64;

    match mode {
        Mode::Capsule => {
            let inside = rx >= 0.0 && ry >= 0.0 && rx <= r.w as f64 && ry <= r.h as f64;
            {
                let n = TICK_N.fetch_add(1, Ordering::Relaxed);
                if n % 20 == 0 {
                    dbg(format!("tick capsule r=({},{},{}x{}) cur=({:.0},{:.0}) rx={:.0} ry={:.0} inside={}",
                        r.x, r.y, r.w, r.h, cur.x, cur.y, rx, ry, inside));
                }
            }
            if inside {
                let due = {
                    let mut g = st();
                    let t = *g.hover_since.get_or_insert(now);
                    now.duration_since(t) >= Duration::from_millis(HOVER_IN)
                };
                if due {
                    expand_ball(app, win);
                }
            } else {
                st().hover_since = None;
            }
        }
        Mode::Ball => {
            let cx = r.w as f64 / 2.0;
            let cy = r.h as f64 / 2.0;
            let inside = (rx - cx).powi(2) + (ry - cy).powi(2) <= (r.w as f64 / 2.0).powi(2);
            if inside {
                let _ = win.set_ignore_cursor_events(false);
                st().leave_since = None;
            } else {
                let _ = win.set_ignore_cursor_events(true); // 圆外的方形区域：点击穿透
                let due = {
                    let mut g = st();
                    let t = *g.leave_since.get_or_insert(now);
                    now.duration_since(t) >= Duration::from_millis(HOVER_OUT)
                };
                if due {
                    collapse(app, win);
                }
            }
        }
        _ => {
            let _ = win.set_ignore_cursor_events(false);
        }
    }
}

pub fn init(app: AppHandle) {
    let p = load_cfg(&app);
    dbg(format!("init cfg docked={} edge={:?} cx={} cy={} visible={}",
        p.docked, p.edge, p.cx, p.cy, p.visible));
    {
        let mut g = st();
        g.edge = p.edge;
        g.opacity = p.opacity;
        g.mode = if p.docked { Mode::Capsule } else { Mode::Free };
    }
    /* 上次退出时悬浮窗是开着的 → 这次启动直接恢复（位置/形态照旧） */
    if p.visible {
        show_ball(&app);
    }
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(60));
        if let Some(win) = app.get_webview_window("pip") {
            tick(&app, &win);
        }
    });
}

/* ---------- 显示 / 隐藏 ---------- */
pub fn toggle(app: &AppHandle) {
    let Some(win) = app.get_webview_window("pip") else { return };
    if win.is_visible().unwrap_or(false) {
        let _ = win.set_ignore_cursor_events(false);
        let _ = win.hide();
        set_visible(app, false);
        return;
    }
    show_ball(app);
}

fn set_visible(app: &AppHandle, v: bool) {
    let mut p = load_cfg(app);
    p.visible = v;
    save_cfg(app, &p);
}

pub fn show_ball(app: &AppHandle) {
    let Some(win) = app.get_webview_window("pip") else { return };
    let p = load_cfg(app);

    if p.docked {
        // 回到上次贴边的位置（沿边方向的中心），越界由 clamp 兜住
        let (cx, cy) = {
            if p.cx >= 0 && p.cy >= 0 {
                (p.cx as f64, p.cy as f64)
            } else {
                let wa = wa_at(app, 0.0, 0.0);
                ((wa.x + wa.w / 2) as f64, (wa.y + wa.h / 2) as f64)
            }
        };
        let wa = wa_at(app, cx, cy);
        let s = scale_at(app, cx, cy);
        let r = capsule_rect(wa, p.edge, s, if matches!(p.edge, Edge::Left | Edge::Right) { cy } else { cx });
        dbg(format!("show_ball docked wa=({},{},{}x{}) s={:.3} cfg_cx={} cfg_cy={} -> r=({},{},{}x{})",
            wa.x, wa.y, wa.w, wa.h, s, p.cx, p.cy, r.x, r.y, r.w, r.h));
        let _ = win.set_size(PhysicalSize::new(r.w as u32, r.h as u32));
        let _ = win.set_position(PhysicalPosition::new(r.x, r.y));
        {
            let mut g = st();
            g.mode = Mode::Capsule;
            g.edge = p.edge;
            g.quiet_until = Some(Instant::now() + Duration::from_millis(400));
        }
    } else if p.free_x >= 0 && p.free_y >= 0 {
        let s = scale_at(app, p.free_x as f64, p.free_y as f64);
        let w = (FREE_W * s).round() as i32;
        let h = (FREE_H * s).round() as i32;
        let _ = win.set_size(PhysicalSize::new(w as u32, h as u32));
        let _ = win.set_position(PhysicalPosition::new(p.free_x, p.free_y));
        st().mode = Mode::Free;
    } else {
        // 首次：主屏右上角
        let wa = wa_at(app, 0.0, 0.0);
        let s = scale_at(app, 0.0, 0.0);
        let w = (FREE_W * s).round() as i32;
        let h = (FREE_H * s).round() as i32;
        let _ = win.set_size(PhysicalSize::new(w as u32, h as u32));
        let _ = win.set_position(PhysicalPosition::new(wa.x + wa.w - w - 16, wa.y + 16));
        st().mode = Mode::Free;
    }
    let _ = win.show();
    set_visible(app, true);
    emit_state(&win);
}

/* ---------- 命令 ---------- */
#[tauri::command]
pub fn ball_state() -> Ev {
    let g = st();
    Ev { mode: g.mode, edge: g.edge, opacity: g.opacity }
}

#[tauri::command]
pub fn ball_drag(app: AppHandle, win: WebviewWindow) {
    {
        let mut g = st();
        g.dragging = true;
        g.hover_since = None;
        g.leave_since = None;
        g.last_move = None;
    }
    let _ = win.start_dragging();
    /* 拖动是原生调用（立即返回），真正的落点判定交给 Moved 事件静默 260ms 后的 tick */
    let _ = app;
}

/* 关菜单：恢复菜单打开前的形态与几何，任何操作前都先调它 */
fn close_menu(win: &WebviewWindow) {
    let pr = {
        let mut g = st();
        if !g.menu {
            return;
        }
        g.menu = false;
        g.mode = g.prev_mode;
        g.prev_rect
    };
    if let Some(r) = pr {
        animate(win, r, 120);
    }
    emit_state(win);
}

#[tauri::command]
pub fn ball_menu(app: AppHandle, win: WebviewWindow, open: bool) {
    if open {
        let (mode, is_menu) = {
            let g = st();
            (g.mode, g.menu)
        };
        if is_menu || mode == Mode::Free {
            return;
        }
        let r = rect_of(&win);
        let (cx, cy) = (r.x as f64 + r.w as f64 / 2.0, r.y as f64 + r.h as f64 / 2.0);
        let wa = wa_at(&app, cx, cy);
        let s = scale_at(&app, cx, cy);
        let mw = (MENU_W * s).round() as i32;
        let mh = (MENU_H * s).round() as i32;
        let x = clamp_i(r.x + r.w / 2 - mw / 2, wa.x, wa.x + wa.w - mw);
        let y = clamp_i(r.y + r.h / 2 - mh / 2, wa.y, wa.y + wa.h - mh);
        {
            let mut g = st();
            g.menu = true;
            g.prev_mode = g.mode;
            g.prev_rect = Some(r);
            g.mode = Mode::Menu;
            g.hover_since = None;
            g.leave_since = None;
        }
        let _ = win.set_ignore_cursor_events(false);
        animate(&win, Rect { x, y, w: mw, h: mh }, 120);
        emit_state(&win);
    } else {
        close_menu(&win);
    }
}

#[tauri::command]
pub fn ball_action(app: AppHandle, win: WebviewWindow, id: String, value: Option<String>) {
    close_menu(&win);
    match id.as_str() {
        "edge" => {
            let edge = match value.as_deref() {
                Some("left") => Edge::Left,
                Some("top") => Edge::Top,
                Some("bottom") => Edge::Bottom,
                _ => Edge::Right,
            };
            dock_to(&app, &win, edge, None);
        }
        "opacity" => {
            let v: u8 = value.as_deref().and_then(|s| s.parse().ok()).unwrap_or(85);
            let v = v.clamp(30, 100);
            st().opacity = v;
            let mut cfg = load_cfg(&app);
            cfg.opacity = v;
            save_cfg(&app, &cfg);
            emit_state(&win);
        }
        "run" => {
            let _ = app.emit("focus-toggle", ());
        }
        "free" => {
            let (cx, cy) = center_of(&win);
            restore_free(&app, &win, cx, cy);
        }
        "hide" => {
            let _ = win.set_ignore_cursor_events(false);
            let _ = win.hide();
            set_visible(&app, false);
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
}
