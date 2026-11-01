#requires -version 5.1
<#
  饼控自律 · 三端同步脚本
  作用：一次跑完「提交 → 推送 → 等 Pages 构建 → 编译 EXE → 三端一致性核对」

  用法：
    双击 同步三端.bat                        （日常用法）
    同步三端.bat "改了什么"                   （自定义提交说明）
    powershell -File sync-three.ps1 -SkipBuild
    powershell -File sync-three.ps1 -AllowRunning   （仅供排查，正常不要用）
#>
param(
  [Parameter(Position = 0)][string]$Message = "",
  [switch]$SkipBuild,
  [switch]$AllowRunning,
  [switch]$CheckOnly
)

$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
try { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch {}

$root = Split-Path -Parent $PSCommandPath
Set-Location -LiteralPath $root

$LOG = Join-Path $root "sync-last.log"
try { Start-Transcript -Path $LOG -Force | Out-Null } catch {}

if (-not ($env:Path -like "*Program Files\nodejs*")) {
  $env:Path = "C:\Program Files\nodejs;$env:USERPROFILE\.cargo\bin;" + $env:Path
}

$LIVE   = "https://bl026732-ship-it.github.io/bingkong-pwa"
$FILES  = @("index.html", "sw.js")
$EXE    = Join-Path $root "src-tauri\target\release\app.exe"
$TMPDIR = Join-Path $env:TEMP "bk_live_check"

function Sep    { Write-Host ("  " + ("-" * 56)) -ForegroundColor DarkGray }
function Head($t) { Write-Host ""; Write-Host $t -ForegroundColor Cyan }
function Ok($m)   { Write-Host "  [OK]   $m" -ForegroundColor Green }
function Bad($m)  { Write-Host "  [FAIL] $m" -ForegroundColor Red }
function Warn($m) { Write-Host "  [WARN] $m" -ForegroundColor Yellow }
function Info($m) { Write-Host "         $m" -ForegroundColor Gray }
function Abort($m) { Bad $m; Write-Host ""; Write-Host "  已中止，未做后续步骤。" -ForegroundColor Yellow; try { Stop-Transcript | Out-Null } catch {}; exit 1 }

Write-Host ""
Write-Host "  饼控自律 · 三端同步" -ForegroundColor White
Write-Host "  源码 → GitHub Pages（桌面/手机网页） → 桌面版 EXE" -ForegroundColor DarkGray
Sep

# ---------- 1. 桌面版是否还在跑 ----------
Head "第 1 步 / 6   检查桌面版是否正在运行"
$procs = @()
try {
  $procs = @(Get-Process -Name app -ErrorAction SilentlyContinue | Where-Object {
    try { $_.Path -and ($_.Path -like "*饼控自律PWA*") } catch { $false }
  })
} catch { $procs = @() }

if ($procs.Count -gt 0 -and -not $AllowRunning -and -not $CheckOnly) {
  Bad "检测到 $($procs.Count) 个 app.exe 正在运行"
  foreach ($p in $procs) {
    $st = try { $p.StartTime.ToString("HH:mm:ss") } catch { "?" }
    Info ("PID {0,-6} 启动于 {1}" -f $p.Id, $st)
  }
  Write-Host ""
  Write-Host "  请先把所有「饼控自律」窗口关掉（包括悬浮窗），再重新双击本脚本。" -ForegroundColor Yellow
  Write-Host "  原因：Windows 不允许覆盖正在运行的 exe，带旧进程编译必然失败。" -ForegroundColor Yellow
  Write-Host "        （2026-11-01 实测报错 WebView2 HRESULT 0x8007139F）" -ForegroundColor DarkGray
  Write-Host ""
  Write-Host "  日志已存到：$LOG" -ForegroundColor DarkGray
  try { Stop-Transcript | Out-Null } catch {}
  exit 1
}
if ($procs.Count -gt 0) { Warn "有 $($procs.Count) 个 app.exe 在跑，已按参数放行（可能编译失败）" }
else { Ok "没有运行中的桌面版" }

# ---------- 2. 提交 ----------
Head "第 2 步 / 6   提交本地改动"
$status = @(git -c core.quotepath=false status --porcelain 2>$null)
if ($CheckOnly) {
  if ($status.Count -eq 0) { Ok "工作区干净" } else { Warn "有 $($status.Count) 项未提交改动"; $status | ForEach-Object { Info $_ } }
}
elseif ($status.Count -eq 0) {
  Ok "工作区干净，无需提交"
}
else {
  Info "本次改动："
  $status | ForEach-Object { Info ("  " + $_) }
  if ([string]::IsNullOrWhiteSpace($Message)) {
    $Message = "同步更新 " + (Get-Date -Format "yyyy-MM-dd HH:mm")
  }
  git add -A 2>&1 | Out-Null
  $commitOut = git commit -m $Message 2>&1
  if ($LASTEXITCODE -ne 0) {
    $commitOut | Select-Object -Last 5 | ForEach-Object { Info $_ }
    Abort "git commit 失败"
  }
  Ok "已提交：$Message"
}

# ---------- 3. 推送 ----------
Head "第 3 步 / 6   推送到 GitHub"
if ($CheckOnly) { Info "检查模式，跳过推送" }
else {
  $pushOut = git push origin main 2>&1
  if ($LASTEXITCODE -ne 0) {
    $pushOut | Select-Object -Last 8 | ForEach-Object { Info $_ }
    Abort "git push 失败（网络或凭据问题）"
  }
  $head = (git rev-parse --short HEAD).Trim()
  Ok "已推送，当前提交 $head"
}

# ---------- 4. 等 Pages 构建 ----------
Head "第 4 步 / 6   等待 GitHub Pages 构建完成"
New-Item -ItemType Directory -Force -Path $TMPDIR | Out-Null
$match = $false
$waited = 0
for ($i = 1; $i -le 30; $i++) {
  $match = $true
  foreach ($f in $FILES) {
    $dst = Join-Path $TMPDIR $f
    try {
      Invoke-WebRequest -Uri "$LIVE/$f" -OutFile $dst -UseBasicParsing -TimeoutSec 20 -Headers @{ "Cache-Control" = "no-cache" } | Out-Null
    } catch {
      $match = $false
      continue
    }
    if (-not (Test-Path $dst)) { $match = $false; continue }
    $hLocal = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $root $f)).Hash
    $hLive  = (Get-FileHash -Algorithm SHA256 -LiteralPath $dst).Hash
    if ($hLocal -ne $hLive) { $match = $false }
  }
  if ($match) { Ok "线上版本已与本地一致（用时约 $waited 秒）"; break }
  $waited += 10
  Info "第 $i/30 次：线上还是旧版，10 秒后重试……"
  Start-Sleep -Seconds 10
}
if (-not $match) {
  Warn "等了 300 秒仍不一致，可能是 Pages 还在构建或构建失败"
  Info "可到 https://github.com/bl026732-ship-it/bingkong-pwa/actions 查看"
}

# ---------- 5. 编译 EXE ----------
Head "第 5 步 / 6   重新编译桌面版 EXE"
if ($SkipBuild -or $CheckOnly) {
  Info "已跳过编译"
}
elseif (-not (Test-Path $EXE)) {
  Warn "没找到旧的 app.exe，将全量编译（可能要 5-10 分钟）"
}
else {
  Info "预计 2 分钟左右，过程中不要关这个窗口"
  $log = & npx tauri build --no-bundle 2>&1
  if ($LASTEXITCODE -ne 0) {
    Bad "编译失败（退出码 $LASTEXITCODE），最后 15 行："
    $log | Select-Object -Last 15 | ForEach-Object { Write-Host "    $_" -ForegroundColor DarkYellow }
    Abort "编译未通过，EXE 仍是旧版本"
  }
  Ok "编译成功"
}

# ---------- 6. 三端核对 ----------
Head "第 6 步 / 6   三端一致性核对"
$rows = @()
foreach ($f in $FILES) {
  $lp = Join-Path $root $f
  $dp = Join-Path $TMPDIR $f
  $hl = if (Test-Path $lp) { (Get-FileHash -Algorithm SHA256 -LiteralPath $lp).Hash.Substring(0, 12) } else { "缺失" }
  $hd = if (Test-Path $dp) { (Get-FileHash -Algorithm SHA256 -LiteralPath $dp).Hash.Substring(0, 12) } else { "未取到" }
  $same = ($hl -eq $hd)
  $rows += [pscustomobject]@{ 文件 = $f; 本地 = $hl; 线上 = $hd; 一致 = $(if ($same) { "是" } else { "否" }) }
}
$rows | Format-Table -AutoSize | Out-String | ForEach-Object { Write-Host $_ -ForegroundColor Gray }

$exeOk = $false
if (Test-Path $EXE) {
  $exeTime = (Get-Item -LiteralPath $EXE).LastWriteTime
  $srcTime = (Get-Item -LiteralPath (Join-Path $root "index.html")).LastWriteTime
  $exeSize = [math]::Round((Get-Item -LiteralPath $EXE).Length / 1MB, 2)
  Info ("EXE  编译于 {0}   {1} MB" -f $exeTime.ToString("yyyy-MM-dd HH:mm:ss"), $exeSize)
  Info ("源码 修改于 {0}" -f $srcTime.ToString("yyyy-MM-dd HH:mm:ss"))
  if ($exeTime -ge $srcTime) { Ok "EXE 比源码新，说明已包含最新改动"; $exeOk = $true }
  else { Bad "EXE 比源码旧，说明没有重新编译"; }
}
else { Bad "找不到 $EXE" }

$webOk = ($rows | Where-Object { $_.一致 -ne "是" }).Count -eq 0

Write-Host ""
Sep
if ($webOk -and $exeOk) {
  Write-Host "  结论：三端一致 ✓" -ForegroundColor Green
  Info "网页端（电脑浏览器 + 手机浏览器）：新版"
  Info "桌面版 EXE：新版，位置 $EXE"
  Info "提醒：如果桌面还开着旧窗口，先全部关掉再双击，否则看到的仍是旧界面。"
}
elseif ($CheckOnly) {
  Write-Host "  结论：当前状态如上（检查模式，未做任何改动）" -ForegroundColor Yellow
}
else {
  Write-Host "  结论：存在不一致，见上方 [FAIL] 项" -ForegroundColor Red
}
Sep
Write-Host "  日志：$LOG" -ForegroundColor DarkGray
Write-Host ""
try { Stop-Transcript | Out-Null } catch {}
