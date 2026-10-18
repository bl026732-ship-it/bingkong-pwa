/* 饼控自律 · 云同步中转站（Cloudflare Worker，module 格式）
 *
 * 部署步骤（Cloudflare 仪表盘操作）：
 * 1. Workers and Pages → Create Worker → 从 Hello World 开始 → Deploy
 * 2. Worker → Settings → Bindings → 新建 KV Namespace 并绑定为 BINGKONG
 * 3. Edit Code：粘贴本文件全部内容，把 SYNC_KEY 改成你自己的密码 → Deploy
 * 4. 浏览器访问 https://你的worker.workers.dev/sync 显示 forbidden = 部署成功
 * 5. 饼控自律 → ☁️ 同步 → 填入 Worker 地址和密钥 → 保存配置
 *
 * 安全说明：没有正确的 X-Sync-Key 一律拒绝（403）；CORS 放行任意域名
 * 是安全的，因为请求必须带密钥头，恶意网站拿不到你的密钥。
 */
const SYNC_KEY = '请改成你自己的密码';

const CORS = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'X-Sync-Key, Content-Type',
  'Access-Control-Allow-Methods': 'GET, POST, OPTIONS'
};

export default {
  async fetch(request, env) {
    /* 浏览器跨域预检 */
    if (request.method === 'OPTIONS') {
      return new Response(null, { status: 204, headers: CORS });
    }

    const url = new URL(request.url);
    if (url.pathname !== '/sync') return new Response('not found', { status: 404, headers: CORS });

    if (request.headers.get('X-Sync-Key') !== SYNC_KEY) {
      return new Response('forbidden', { status: 403, headers: CORS });
    }

    if (request.method === 'GET') {
      const data = await env.BINGKONG.get('state');
      return new Response(data || '{}', {
        headers: Object.assign({}, CORS, { 'Content-Type': 'application/json' })
      });
    }

    if (request.method === 'POST') {
      const body = await request.text();
      if (body.length > 500000) return new Response('too large', { status: 413, headers: CORS });
      await env.BINGKONG.put('state', body);
      return new Response('{"ok":true}', {
        headers: Object.assign({}, CORS, { 'Content-Type': 'application/json' })
      });
    }

    return new Response('method not allowed', { status: 405, headers: CORS });
  }
};
