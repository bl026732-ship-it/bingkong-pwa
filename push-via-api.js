#!/usr/bin/env node
/*
  饼控自律 · 推送兜底（GitHub REST API 通道）

  背景：github.com 直连/代理偶发不通（实测 CONNECT tunnel 502、直连超时），
        而 api.github.com 一直可达。此脚本用 Git Data API 复刻当前 HEAD 提交，
        把同一份内容推到远端，并让本地与远端最终落在同一个提交 SHA 上。

  用法：
        GITHUB_TOKEN=xxx node push-via-api.js
        GITHUB_TOKEN=xxx node push-via-api.js --dry-run   （只生成对象、不移动分支，用于自检）

  退出码：0 成功或已是最新 / 1 失败 / 2 需要人工处理
  由 sync-three.ps1 在 git push 失败时自动调用，一般不需要手动运行。
*/
'use strict';

const { execFileSync, spawnSync } = require('child_process');
const crypto = require('crypto');
const path = require('path');

const ROOT = __dirname;
const OWNER = process.env.GH_OWNER || 'bl026732-ship-it';
const REPO = process.env.GH_REPO || 'bingkong-pwa';
const BRANCH = process.env.GH_BRANCH || 'main';
const TOKEN = process.env.GITHUB_TOKEN;
const DRY = process.argv.includes('--dry-run');
const API = 'https://api.github.com';

function log(m) { console.log('         ' + m); }
function bad(m) { console.error('  [FAIL] ' + m); }

function git(args, opts) {
  return execFileSync('git', args, Object.assign({
    cwd: ROOT, encoding: 'utf8', maxBuffer: 128 * 1024 * 1024
  }, opts || {})).replace(/\s+$/, '');
}
function gitBuf(args) {
  return execFileSync('git', args, { cwd: ROOT, maxBuffer: 256 * 1024 * 1024 });
}

const H = {
  Authorization: 'Bearer ' + (TOKEN || ''),
  Accept: 'application/vnd.github+json',
  'X-GitHub-Api-Version': '2022-11-28',
  'Content-Type': 'application/json',
  'User-Agent': 'bingkong-sync'
};

async function api(method, url, body) {
  let last;
  for (let i = 0; i < 2; i++) {
    try {
      const r = await fetch(API + url, {
        method, headers: H, body: body ? JSON.stringify(body) : undefined,
        signal: AbortSignal.timeout(45000)
      });
      const t = await r.text();
      if (!r.ok) throw new Error(method + ' ' + url + ' -> ' + r.status + ' ' + t.slice(0, 400));
      return t ? JSON.parse(t) : null;
    } catch (e) {
      last = e;
      if (i === 0) await new Promise(r => setTimeout(r, 1200));
    }
  }
  throw last;
}

/* 把 "2026-11-01 20:27:48 +0800" 转成 ISO 带时区偏移，避免服务器把时区归一化成 +0000 而改变对象 SHA */
function toIsoWithTz(ai) {
  const m = ai.match(/^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2}) ([+-])(\d{2})(\d{2})$/);
  if (!m) return null;
  const [, y, mo, d, hh, mm, ss, sign, tzh, tzm] = m;
  return `${y}-${mo}-${d}T${hh}:${mm}:${ss}${sign}${tzh}:${tzm}`;
}

(async () => {
  if (!TOKEN) { bad('缺少 GITHUB_TOKEN'); process.exit(1); }

  const localHead = git(['rev-parse', 'HEAD']);
  const localTree = git(['show', '-s', '--format=%T', 'HEAD']);
  const raw = git(['log', '-1', '--format=%an%x00%ae%x00%at%x00%ai%x00%cn%x00%ce%x00%ct%x00%ci%x00%B', 'HEAD']);
  const parts = raw.split('\u0000');
  const aName = parts[0], aMail = parts[1], aEpoch = parts[2], aAi = parts[3];
  const cName = parts[4], cMail = parts[5], cEpoch = parts[6], cCi = parts[7];
  const message = parts.slice(8).join('\u0000').replace(/[\r\n]+$/, ''); // 服务器存储时不留末尾换行

  log('本地 HEAD = ' + localHead.slice(0, 10));
  log('说明     = ' + message.split('\n')[0]);

  const ref = await api('GET', `/repos/${OWNER}/${REPO}/git/ref/heads/${BRANCH}`);
  const remoteHead = ref.object.sha;
  if (remoteHead === localHead) { log('远端已经是这个提交，无需推送'); console.log('RESULT=UP_TO_DATE'); process.exit(0); }
  log('远端 HEAD = ' + remoteHead.slice(0, 10));

  const remoteCommit = await api('GET', `/repos/${OWNER}/${REPO}/git/commits/${remoteHead}`);
  const remoteTree = remoteCommit.tree.sha;

  /* 远端全量清单（不依赖本地是否有远端对象） */
  const remoteTreeResp = await api('GET', `/repos/${OWNER}/${REPO}/git/trees/${remoteTree}?recursive=1`);
  if (remoteTreeResp.truncated) { bad('远端文件过多，递归清单被截断，保守起见不再继续'); process.exit(2); }
  const remoteMap = new Map();
  for (const e of remoteTreeResp.tree) remoteMap.set(e.path, e);

  /* 本地全量清单 */
  const localMap = new Map();
  for (const line of git(['-c', 'core.quotepath=false', 'ls-tree', '-r', 'HEAD']).split('\n')) {
    if (!line.trim()) continue;
    const tab = line.indexOf('\t');
    const meta = line.slice(0, tab).split(/\s+/);
    localMap.set(line.slice(tab + 1), { mode: meta[0], type: meta[1], sha: meta[2] });
  }

  const entries = [];
  let uploaded = 0, removed = 0;

  for (const [p, info] of localMap) {
    const r = remoteMap.get(p);
    const same = r && r.sha === info.sha && r.mode === info.mode && r.type === info.type;
    if (same) continue;
    if (info.type !== 'blob') { bad('暂不支持非普通文件：' + p); process.exit(2); }
    const buf = gitBuf(['show', 'HEAD:' + p]);
    const blob = await api('POST', `/repos/${OWNER}/${REPO}/git/blobs`, {
      content: buf.toString('base64'), encoding: 'base64'
    });
    if (blob.sha !== info.sha) { bad('上传后 blob 校验失败：' + p + ' 本地 ' + info.sha.slice(0, 8) + ' 远端 ' + blob.sha.slice(0, 8)); process.exit(1); }
    entries.push({ path: p, mode: info.mode, type: 'blob', sha: blob.sha });
    uploaded++;
    log('上传 ' + p);
  }

  for (const [p, e] of remoteMap) {
    if (e.type !== 'blob') continue;
    if (localMap.has(p)) continue;
    entries.push({ path: p, mode: e.mode, type: 'blob', sha: null });
    removed++;
    log('删除 ' + p);
  }

  log(`变更：新增/修改 ${uploaded} 个，删除 ${removed} 个`);

  const tree = await api('POST', `/repos/${OWNER}/${REPO}/git/trees`, { base_tree: remoteTree, tree: entries });
  const treeMatchesLocal = tree.sha === localTree;
  log('新 tree  = ' + tree.sha.slice(0, 10) + (treeMatchesLocal ? '（与本地一致）' : '（与本地不同，远端有其他改动）'));

  const commit = await api('POST', `/repos/${OWNER}/${REPO}/git/commits`, {
    message,
    tree: tree.sha,
    parents: [remoteHead],
    author: { name: aName, email: aMail, date: toIsoWithTz(aAi) || undefined },
    committer: { name: cName, email: cMail, date: toIsoWithTz(cCi) || undefined }
  });
  log('新提交   = ' + commit.sha.slice(0, 10));

  if (DRY) { log('--dry-run：不移动分支，远端保持原样'); console.log('RESULT=DRY_OK'); process.exit(0); }

  await api('PATCH', `/repos/${OWNER}/${REPO}/git/refs/heads/${BRANCH}`, { sha: commit.sha, force: false });
  const after = await api('GET', `/repos/${OWNER}/${REPO}/git/ref/heads/${BRANCH}`);
  if (after.object.sha !== commit.sha) { bad('分支更新后校验不一致：' + after.object.sha); process.exit(1); }
  log('远端 ' + BRANCH + ' 已指向 ' + commit.sha.slice(0, 10));

  /* 让本地也落在同一个提交上，避免下次 git push 变成 non-fast-forward */
  if (!treeMatchesLocal) {
    log('提示：远端包含本地没有的改动，本地未自动对齐，稍后请在有网时执行 git fetch + 合并');
    console.log('RESULT=PUSHED_DIVERGED');
    process.exit(0);
  }

  const tzA = aAi.slice(-5), tzC = cCi.slice(-5);
  const obj = [
    'tree ' + tree.sha,
    'parent ' + remoteHead,
    `author ${aName} <${aMail}> ${aEpoch} ${tzA}`,
    `committer ${cName} <${cMail}> ${cEpoch} ${tzC}`,
    '',
    message
  ].join('\n');

  const computed = crypto.createHash('sha1')
    .update('commit ' + Buffer.byteLength(obj, 'utf8') + '\u0000' + obj, 'utf8')
    .digest('hex');

  if (computed !== commit.sha) {
    log('提示：本地重建的提交对象与远端 SHA 不同（' + computed.slice(0, 10) + ' vs ' + commit.sha.slice(0, 10) + '），内容一致但不自动移动本地分支');
    console.log('RESULT=PUSHED_SHA_DIFF');
    process.exit(0);
  }

  const written = spawnSync('git', ['hash-object', '-t', 'commit', '-w', '--stdin'], {
    cwd: ROOT, input: Buffer.from(obj, 'utf8'), encoding: 'utf8'
  });
  if (written.status !== 0) { bad('写入本地提交对象失败：' + (written.stderr || '').slice(0, 200)); process.exit(1); }
  const localSha = (written.stdout || '').trim();
  if (localSha !== commit.sha) { bad('本地写入的 SHA 与远端不一致：' + localSha); process.exit(1); }

  /* HEAD 当前指向旧提交，这里把它移到同一个提交上；工作区内容不变（tree 相同） */
  git(['update-ref', 'refs/heads/' + BRANCH, commit.sha]);
  git(['update-ref', 'refs/remotes/origin/' + BRANCH, commit.sha]);
  log('本地与远端已对齐到同一提交 ' + commit.sha.slice(0, 10));
  console.log('RESULT=OK');
  console.log('SHA=' + commit.sha);
})().catch(e => { bad(e.message || String(e)); process.exit(1); });
