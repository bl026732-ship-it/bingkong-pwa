/* 饼控自律 · 云同步中转站（Cloudflare Worker）
 *
 * 部署步骤（在 Cloudflare 仪表盘操作，无需写 wrangler）：
 * 1. 左侧菜单 Workers and Pages → Create Worker → Deploy（先建一个空 Worker）
 * 2. 进入这个 Worker → Settings → Variables and KV Namespace Bindings
 *    → Add binding → 类型选 KV Namespace，新建一个名为 BINGKONG 的空间，绑定名填 BINGKONG
 * 3. 回到 Worker 的 Edit Code，把下面整段代码粘进去，把 SYNC_KEY 改成你自己的密码
 * 4. Deploy。右上角复制 Worker 的 URL（https://xxx.yyy.workers.dev）
 * 5. 打开饼控自律 → ☁️ 同步 → 填入 Worker 地址和同样的密钥 → 保存配置
 *
 * 安全说明：没有正确的 X-Sync-Key 请求会被拒绝（403），数据只存在你的 KV 空间里。
 */
const SYNC_KEY = '请改成你自己的密码';

addEventListener('fetch', event => {
  event.respondWith(handleRequest(event.request));
});

async function handleRequest(request) {
  const url = new URL(request.url);
  if (url.pathname !== '/sync') return new Response('not found', { status: 404 });

  if (request.headers.get('X-Sync-Key') !== SYNC_KEY) {
    return new Response('forbidden', { status: 403 });
  }

  if (request.method === 'GET') {
    const data = await BINGKONG.get('state');
    return new Response(data || '{}', {
      headers: { 'Content-Type': 'application/json' }
    });
  }

  if (request.method === 'POST') {
    const body = await request.text();
    if (body.length > 500000) return new Response('too large', { status: 413 });
    await BINGKONG.put('state', body);
    return new Response('{"ok":true}', {
      headers: { 'Content-Type': 'application/json' }
    });
  }

  return new Response('method not allowed', { status: 405 });
}
